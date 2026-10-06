//! Disposable machine integration only. Never restore a whole registry/store.
use devone::{
    app::Application,
    core::{Result, fail},
    tls::{self, CaddyTls, TlsProvider},
};
use std::{collections::BTreeSet, path::PathBuf};
static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SIGNAL_HANDLER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub fn user_roots() -> Result<BTreeSet<String>> {
    use winreg::{RegKey, enums::*};
    let key = match RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\SystemCertificates\Root\Certificates")
    {
        Ok(key) => key,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(e) => return Err(e.into()),
    };
    key.enum_keys()
        .collect::<std::io::Result<_>>()
        .map_err(Into::into)
}

pub struct MachineGuard {
    pub app: Application,
    certificates: Vec<(PathBuf, String)>,
    roots: BTreeSet<String>,
    cleaned: bool,
    dns_created: bool,
}
impl MachineGuard {
    pub fn new(app: Application) -> Result<Self> {
        if !*SIGNAL_HANDLER.get_or_init(|| ctrlc::set_handler(|| {
            CANCELLED.store(true,std::sync::atomic::Ordering::Release);
            eprintln!("USER CANCELLED: native acceptance will finish exact cleanup before exit; dismiss any Windows confirmation");
        }).is_ok()) { return fail("Cannot install acceptance cancellation cleanup handler; no machine mutation authorized"); }
        Ok(Self {
            app,
            certificates: vec![],
            roots: user_roots()?,
            cleaned: false,
            dns_created: false,
        })
    }
    pub fn cancelled(&self) -> bool {
        CANCELLED.load(std::sync::atomic::Ordering::Acquire)
    }
    /// Register before trust installation, so partial installation is covered.
    /// A private immutable copy survives CA recreation or a changed source file.
    pub fn track_ca(&mut self) -> Result<()> {
        let source = CaddyTls.ca_path(&self.app.home);
        if devone::platform::ca_trusted(&source) {
            return fail("Acceptance must not take ownership of an already trusted CA");
        }
        let copy = self.app.home.path(&format!(
            "cache/acceptance-owned-{}.crt",
            uuid::Uuid::new_v4()
        ));
        std::fs::copy(&source, &copy)?;
        let fingerprint = tls::fingerprint(&copy)?;
        eprintln!(
            "Disposable CA for this test only: {} file SHA-256 {}",
            source.display(),
            fingerprint
        );
        self.certificates.push((copy, fingerprint));
        Ok(())
    }
    /// Call before installation, only after verifying the fixed production key
    /// was absent. This guard must never acquire an existing user's integration.
    pub fn track_dns(&mut self) -> Result<()> {
        use winreg::{RegKey, enums::*};
        let path = r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\DnsPolicyConfig\{9073AE66-0413-46F2-9F35-D0E001000001}";
        if RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(path).is_ok() {
            return fail("Existing DEVONE DNS integration is not owned by this test");
        }
        self.dns_created = true;
        Ok(())
    }
    pub fn cleanup(&mut self) -> Result<()> {
        if self.cleaned {
            return Ok(());
        }
        let mut failures = vec![];
        if let Err(e) = self.app.shutdown() {
            failures.push(e.to_string());
        }
        if self.dns_created {
            use winreg::{RegKey, enums::*};
            let path = r"SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\DnsPolicyConfig\{9073AE66-0413-46F2-9F35-D0E001000001}";
            if RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(path).is_ok()
                && let Err(e) = devone::platform::configure_wildcard_elevated(true)
            {
                failures.push(e.to_string());
            }
            if RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(path).is_ok() {
                failures.push("Test-owned DNS policy remains installed".into());
            }
        }
        for (path, expected) in &self.certificates {
            if !tls::fingerprint(path).is_ok_and(|actual| actual == *expected) {
                failures.push("Acceptance CA copy identity changed; refusing removal".into());
                continue;
            }
            if let Err(e) = devone::platform::remove_ca_trust(path) {
                failures.push(e.to_string());
            }
            if devone::platform::ca_trusted(path) {
                failures.push("Test-owned CA remains trusted".into());
            }
        }
        if user_roots()? != self.roots {
            failures.push("Current-user Root baseline changed".into());
        }
        if !failures.is_empty() {
            return fail(failures.join("; "));
        }
        self.cleaned = true;
        eprintln!(
            "Acceptance cleanup verified: owned resources stopped/removed; current-user Root baseline unchanged"
        );
        Ok(())
    }
}
impl Drop for MachineGuard {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            if std::thread::panicking() {
                eprintln!("ACCEPTANCE CLEANUP FAILED: {e}");
            } else {
                panic!("ACCEPTANCE CLEANUP FAILED: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devone::{app::Options, config::Home};
    #[test]
    fn untrusted_guard_cleanup_is_noop_and_changed_capture_is_refused() {
        let before = user_roots().unwrap();
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        let app = Application::open_with_options(
            home.clone(),
            Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        let path = CaddyTls.ca_path(&home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = include_bytes!("../fixtures/ca-owned.crt");
        std::fs::write(&path, original).unwrap();
        assert!(
            !devone::platform::ca_trusted(&path),
            "Public fixture unexpectedly trusted; refuse to acquire it"
        );
        let mut guard = MachineGuard::new(app).unwrap();
        guard.track_ca().unwrap();
        let capture = guard.certificates[0].0.clone();
        std::fs::write(&capture, b"unknown changed identity").unwrap();
        assert!(
            guard
                .cleanup()
                .unwrap_err()
                .to_string()
                .contains("refusing removal")
        );
        assert_eq!(user_roots().unwrap(), before);
        std::fs::write(&capture, original).unwrap();
        guard.cleanup().unwrap();
        guard.cleanup().unwrap();
        assert_eq!(user_roots().unwrap(), before);
    }
}
