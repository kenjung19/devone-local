use crate::{
    config::Home,
    core::{Result, fail},
    storage::Store,
};
pub trait TlsProvider {
    fn ca_path(&self, home: &Home) -> std::path::PathBuf;
    fn trust(&self, store: &Store, home: &Home) -> Result<()>;
}
pub struct CaddyTls;
pub fn fingerprint(path: &std::path::Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(Sha256::digest(std::fs::read(path)?)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub fn verify_owned(store: &Store, home: &Home) -> Result<()> {
    let actual = fingerprint(&CaddyTls.ca_path(home))?;
    if store.setting("tls.ca_fingerprint")?.as_deref() != Some(&actual) {
        return fail(
            "CA identity does not match DEVONE's recorded trust ownership; no certificate was removed",
        );
    }
    Ok(())
}
pub fn remove_trust(store: &Store, home: &Home) -> Result<()> {
    let path = CaddyTls.ca_path(home);
    if store
        .setting("tls.ca_fingerprint")?
        .is_some_and(|s| !s.is_empty())
    {
        verify_owned(store, home)?;
    }
    if crate::platform::ca_trusted(&path) {
        verify_owned(store, home)?;
        crate::platform::remove_ca_trust(&path)?;
        if crate::platform::ca_trusted(&path) {
            return fail("DEVONE CA trust is still present; retry removal");
        }
    }
    store.set_setting("tls.ca_fingerprint", "")?;
    store.conn.execute(
        "UPDATE certificates SET trusted=0 WHERE id='caddy-local'",
        [],
    )?;
    Ok(())
}
pub fn recreate(store: &Store, home: &Home, confirmed: bool) -> Result<()> {
    if !confirmed {
        return fail(
            "Confirm CA recreation: existing generated HTTPS certificates must be regenerated",
        );
    }
    let storage = home.path("certs/caddy");
    if storage.exists() && crate::platform::is_link(&storage)? {
        return fail("CA storage is a link; refusing to move it");
    }
    let previously_trusted = crate::platform::ca_trusted(&CaddyTls.ca_path(home));
    remove_trust(store, home)?;
    let backup = home
        .path("backups")
        .join(format!("caddy-ca-{}", uuid::Uuid::new_v4()));
    let moved = storage.exists();
    if moved {
        std::fs::rename(&storage, &backup)?;
    }
    if let Err(error) = crate::setup::prepare_ca(store, home) {
        // Preserve partial output as well as the old CA, without deleting data.
        if storage.exists() {
            std::fs::rename(
                &storage,
                home.path("backups")
                    .join(format!("failed-ca-{}", uuid::Uuid::new_v4())),
            )?;
        }
        if moved {
            std::fs::rename(&backup, &storage)?;
        }
        if previously_trusted {
            CaddyTls.trust(store, home)?;
        }
        return Err(error);
    }
    Ok(())
}
impl TlsProvider for CaddyTls {
    fn ca_path(&self, home: &Home) -> std::path::PathBuf {
        home.path("certs/caddy/pki/authorities/local/root.crt")
    }
    fn trust(&self, store: &Store, home: &Home) -> Result<()> {
        let path = self.ca_path(home);
        if !path.is_file() {
            return fail("Caddy CA is not generated yet. Start a configured site first.");
        }
        let fingerprint = fingerprint(&path)?;
        if store
            .setting("tls.ca_fingerprint")?
            .is_some_and(|recorded| !recorded.is_empty() && recorded != fingerprint)
        {
            return fail(
                "CA identity changed. Restore the recorded CA before changing trust; existing ownership was preserved",
            );
        }
        if store.setting("tls.ca_fingerprint")?.as_deref() == Some(&fingerprint)
            && crate::platform::ca_trusted(&path)
        {
            return Ok(());
        }
        // Record exact ownership before the OS mutation. A cancelled/timed-out
        // installation can still have added the root, so removal must remain
        // possible even when the installation command reports an error.
        store.set_setting("tls.ca_fingerprint", &fingerprint)?;
        crate::platform::trust_ca(&path)?;
        if !crate::platform::ca_trusted(&path) {
            return fail(
                "The CA is not present in the current user trust store after installation",
            );
        }
        store.set_setting("tls.ca_fingerprint", &fingerprint)?;
        store.conn.execute("INSERT INTO certificates(id,path,trusted) VALUES('caddy-local',?1,1) ON CONFLICT(id) DO UPDATE SET path=excluded.path,trusted=1",[path.to_string_lossy().to_string()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn certificate_removal_requires_recorded_exact_identity_and_recreate_confirmation() {
        let d = tempfile::tempdir().unwrap();
        let h = Home::new(d.path());
        h.ensure().unwrap();
        let s = Store::open(&h.path("state.db")).unwrap();
        let p = CaddyTls.ca_path(&h);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"owned CA identity").unwrap();
        assert!(verify_owned(&s, &h).is_err());
        s.set_setting("tls.ca_fingerprint", &fingerprint(&p).unwrap())
            .unwrap();
        verify_owned(&s, &h).unwrap();
        std::fs::write(&p, b"foreign replacement").unwrap();
        assert!(verify_owned(&s, &h).is_err());
        let recorded = s.setting("tls.ca_fingerprint").unwrap();
        assert!(CaddyTls.trust(&s, &h).is_err());
        assert_eq!(s.setting("tls.ca_fingerprint").unwrap(), recorded);
        assert!(recreate(&s, &h, false).is_err());
        assert_eq!(std::fs::read(p).unwrap(), b"foreign replacement");
    }
}
