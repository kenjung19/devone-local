//! The constraints are the control for a stolen key. File ACLs cannot prevent
//! a process running as this same Windows user from reading the private key.
use crate::{
    config::Home,
    core::{Error, Result, fail},
};
use rcgen::PublicKeyData;
use rcgen::{
    BasicConstraints, CertificateParams, CidrSubnet, DnType, GeneralSubtree, IsCa, KeyPair,
    KeyUsagePurpose, NameConstraints, PKCS_ECDSA_P256_SHA256,
};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use x509_parser::{
    extensions::{GeneralName, ParsedExtension},
    pem::parse_x509_pem,
    prelude::{FromDer, X509Certificate},
};

pub fn root_path(home: &Home) -> PathBuf {
    home.path("certs/devone-ca/root.crt")
}
pub fn key_path(home: &Home) -> PathBuf {
    home.path("certs/devone-ca/root.key")
}
pub(crate) fn safe_path(home: &Home, path: &Path) -> Result<()> {
    if !path.starts_with(home.root()) {
        return fail("CA path escapes Home");
    }
    let mut current = Some(path);
    while let Some(p) = current {
        match std::fs::symlink_metadata(p) {
            Ok(_) if crate::platform::is_link(p)? => {
                return fail("CA path is a link/junction; refusing to change it");
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if p == home.root() {
            break;
        }
        current = p.parent();
    }
    Ok(())
}
pub(super) fn parameters() -> Result<CertificateParams> {
    let mut params = CertificateParams::default();
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(3652);
    params.distinguished_name.push(
        DnType::CommonName,
        format!("DEVONE Local CA {}", uuid::Uuid::new_v4()),
    );
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(1));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.name_constraints = Some(NameConstraints {
        permitted_subtrees: vec![GeneralSubtree::DnsName("test".into())],
        excluded_subtrees: vec![
            GeneralSubtree::IpAddress(CidrSubnet::V4([0; 4], [0; 4])),
            GeneralSubtree::IpAddress(CidrSubnet::V6([0; 16], [0; 16])),
        ],
    });
    Ok(params)
}
pub fn validate(home: &Home) -> Result<()> {
    let cert_path = root_path(home);
    let key_path = key_path(home);
    safe_path(home, &cert_path)?;
    safe_path(home, &key_path)?;
    let bytes = std::fs::read(&cert_path)?;
    let (_, pem) =
        parse_x509_pem(&bytes).map_err(|e| Error::Message(format!("Invalid DEVONE root: {e}")))?;
    let (_, cert) = X509Certificate::from_der(&pem.contents)
        .map_err(|e| Error::Message(format!("Invalid DEVONE root: {e}")))?;
    let mut basic = false;
    let mut usage = false;
    let mut constraints = false;
    for extension in cert.extensions() {
        match extension.parsed_extension() {
            ParsedExtension::BasicConstraints(value) => {
                basic = extension.critical && value.ca && value.path_len_constraint == Some(1)
            }
            ParsedExtension::KeyUsage(value) => usage = extension.critical && value.flags == 0x60,
            ParsedExtension::NameConstraints(value) => {
                let permitted = value.permitted_subtrees.as_deref().unwrap_or_default();
                let excluded = value.excluded_subtrees.as_deref().unwrap_or_default();
                constraints = extension.critical
                    && permitted.len() == 1
                    && matches!(permitted[0].base, GeneralName::DNSName("test"))
                    && excluded.len() == 2
                    && excluded
                        .iter()
                        .any(|s| matches!(s.base, GeneralName::IPAddress(v) if v == [0; 8]))
                    && excluded
                        .iter()
                        .any(|s| matches!(s.base, GeneralName::IPAddress(v) if v == [0; 32]));
            }
            _ => {}
        }
    }
    let key_pem = zeroize::Zeroizing::new(std::fs::read_to_string(key_path)?);
    let key = KeyPair::from_pem(&key_pem).map_err(|e| Error::Message(e.to_string()))?;
    if !(basic && usage && constraints)
        || cert.subject() != cert.issuer()
        || key.algorithm() != &PKCS_ECDSA_P256_SHA256
        || key.subject_public_key_info() != cert.public_key().raw
    {
        return fail(
            "DEVONE CA must be a matching P-256 root with critical .test-only name constraints, no IPs, pathLen 1 and signing key usage",
        );
    }
    Ok(())
}
pub fn ensure(home: &Home) -> Result<()> {
    let certs = home.path("certs");
    safe_path(home, &certs)?;
    for entry in std::fs::read_dir(&certs)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".devone-ca-")
            && !crate::platform::is_link(&entry.path())?
            && entry.file_type()?.is_dir()
        {
            // Check descendants too: recursive removal must never traverse a link.
            if let Err(error) = remove_staging(home, &entry.path()) {
                tracing::warn!(%error, path=%entry.path().display(), "abandoned CA staging cleanup skipped");
            }
        }
    }
    let directory = home.path("certs/devone-ca");
    safe_path(home, &directory)?;
    if directory.exists() {
        validate(home)?;
        crate::platform::harden_ca_path(&directory)?;
        return crate::platform::harden_ca_path(&key_path(home));
    }
    let staging = home
        .path("certs")
        .join(format!(".devone-ca-{}", uuid::Uuid::new_v4()));
    safe_path(home, &staging)?;
    std::fs::create_dir(&staging)?;
    crate::platform::harden_ca_path(&staging)?;
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
        .map_err(|e| Error::Message(e.to_string()))?;
    let cert = parameters()?
        .self_signed(&key)
        .map_err(|e| Error::Message(e.to_string()))?;
    // atomic_write uses a create_new temporary file. Publish the pair together
    // using a fresh directory; no existing root/private key is overwritten.
    crate::runtime::atomic_write(&staging.join("root.crt"), cert.pem().as_bytes())?;
    let key_file = staging.join("root.key");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&key_file)?;
    crate::platform::harden_ca_path(&key_file)?;
    let pem = zeroize::Zeroizing::new(key.serialize_pem());
    file.write_all(pem.as_bytes())?;
    file.sync_all()?;
    drop(file);
    if directory.exists() {
        return fail("DEVONE CA already exists; generated staging pair was preserved");
    }
    std::fs::rename(staging, directory)?;
    validate(home)
}
fn remove_staging(home: &Home, path: &Path) -> Result<()> {
    safe_path(home, path)?;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        safe_path(home, &entry.path())?;
        if entry.file_type()?.is_dir() {
            remove_staging(home, &entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    std::fs::remove_dir(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn undeletable_stale_staging_does_not_block_ca_ensure() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        let stale = home.path("certs/.devone-ca-locked");
        std::fs::create_dir(&stale).unwrap();
        let key = stale.join("root.key");
        std::fs::write(&key, "locked stale key").unwrap();
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&key)
            .unwrap();
        ensure(&home).unwrap();
        assert!(key.exists());
        validate(&home).unwrap();
        drop(handle);
        ensure(&home).unwrap();
        assert!(!stale.exists());
    }
    #[test]
    fn ensure_removes_abandoned_private_key_staging_only() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        let abandoned = home.path("certs/.devone-ca-abandoned");
        std::fs::create_dir(&abandoned).unwrap();
        std::fs::write(abandoned.join("root.key"), "orphaned secret").unwrap();
        let unrelated = home.path("certs/other");
        std::fs::create_dir(&unrelated).unwrap();
        ensure(&home).unwrap();
        assert!(!abandoned.exists());
        assert!(unrelated.is_dir());
        assert!(key_path(&home).is_file());
    }
    #[test]
    fn constrained_root_parses_and_existing_key_is_not_overwritten() {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        ensure(&home).unwrap();
        validate(&home).unwrap();
        let bytes = std::fs::read(root_path(&home)).unwrap();
        let (_, pem) = parse_x509_pem(&bytes).unwrap();
        let (_, cert) = X509Certificate::from_der(&pem.contents).unwrap();
        assert!(
            cert.subject()
                .iter_common_name()
                .next()
                .unwrap()
                .as_str()
                .unwrap()
                .starts_with("DEVONE Local CA ")
        );
        assert!(
            cert.validity().not_after.timestamp() - cert.validity().not_before.timestamp()
                >= 3650 * 86400
        );
        let original = std::fs::read(key_path(&home)).unwrap();
        ensure(&home).unwrap();
        assert_eq!(std::fs::read(key_path(&home)).unwrap(), original);
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(key_path(&home))
                .is_err()
        );
        std::fs::write(key_path(&home), "do not overwrite").unwrap();
        assert!(ensure(&home).is_err());
        assert_eq!(
            std::fs::read_to_string(key_path(&home)).unwrap(),
            "do not overwrite"
        );
    }
}
#[cfg(all(test, windows))]
mod windows_tests;
