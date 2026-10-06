use crate::{
    config::Home,
    core::{Result, fail},
    storage::Store,
};
pub mod authority;
mod upgrade;
pub use upgrade::{Reload, resume_safe_pending, upgrade, upgrade_pending};
pub fn legacy_path(home: &Home) -> std::path::PathBuf {
    home.path("certs/caddy/pki/authorities/local/root.crt")
}
pub fn ensure_ca(home: &Home) -> Result<()> {
    if CaddyTls.ca_path(home) == authority::root_path(home) {
        authority::ensure(home)?;
    }
    Ok(())
}
pub trait TlsProvider {
    fn ca_path(&self, home: &Home) -> std::path::PathBuf;
    fn trust(&self, store: &Store, home: &Home) -> Result<()>;
}
pub struct CaddyTls;
trait TrustStore {
    fn trusted(&self, path: &std::path::Path) -> bool;
    fn install(&self, path: &std::path::Path) -> Result<()>;
    fn remove(&self, path: &std::path::Path) -> Result<()>;
}
struct WindowsTrust;
impl TrustStore for WindowsTrust {
    fn trusted(&self, path: &std::path::Path) -> bool {
        crate::platform::ca_trusted(path)
    }
    fn install(&self, path: &std::path::Path) -> Result<()> {
        crate::platform::trust_ca(path)
    }
    fn remove(&self, path: &std::path::Path) -> Result<()> {
        crate::platform::remove_ca_trust(path)
    }
}
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
    remove_with(store, home, &WindowsTrust)
}
fn remove_with(store: &Store, home: &Home, system: &impl TrustStore) -> Result<()> {
    let path = CaddyTls.ca_path(home);
    if store
        .setting("tls.ca_fingerprint")?
        .is_some_and(|s| !s.is_empty())
    {
        verify_owned(store, home)?;
    }
    if system.trusted(&path) {
        verify_owned(store, home)?;
        system.remove(&path)?;
        if system.trusted(&path) {
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
    if upgrade_pending(store, home)? {
        return fail("Upgrade the legacy HTTPS certificate authority before recreating it");
    }
    let storage = home.path("certs/caddy");
    let authority = home.path("certs/devone-ca");
    authority::safe_path(home, &storage)?;
    authority::safe_path(home, &authority)?;
    let previously_trusted = crate::platform::ca_trusted(&CaddyTls.ca_path(home));
    remove_trust(store, home)?;
    let backup = home
        .path("backups")
        .join(format!("caddy-ca-{}", uuid::Uuid::new_v4()));
    authority::safe_path(home, &backup)?;
    std::fs::create_dir(&backup)?;
    crate::platform::harden_ca_path(&backup)?;
    let moved_authority = authority.exists();
    if moved_authority {
        std::fs::rename(&authority, backup.join("devone-ca"))?;
    }
    let moved = storage.exists();
    if moved {
        std::fs::rename(&storage, backup.join("caddy"))?;
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
            std::fs::rename(backup.join("caddy"), &storage)?;
        }
        if authority.exists() {
            std::fs::rename(&authority, backup.join("failed-devone-ca"))?;
        }
        if moved_authority {
            std::fs::rename(backup.join("devone-ca"), &authority)?;
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
        // A staged root does not activate a legacy upgrade without user consent.
        if legacy_path(home).exists() && !home.path("certs/devone-ca/active").exists() {
            legacy_path(home)
        } else {
            authority::root_path(home)
        }
    }
    fn trust(&self, store: &Store, home: &Home) -> Result<()> {
        if self.ca_path(home) == authority::root_path(home) {
            authority::validate(home)?;
        }
        trust_with(store, home, &WindowsTrust)
    }
}
fn trust_with(store: &Store, home: &Home, system: &impl TrustStore) -> Result<()> {
    let path = CaddyTls.ca_path(home);
    if !path.is_file() {
        return fail("DEVONE CA is not generated yet. Prepare HTTPS first.");
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
        && system.trusted(&path)
    {
        return record_trusted(store, &path);
    }
    // Record exact ownership before the OS mutation. A cancelled/timed-out
    // installation can still have added the root, so removal must remain
    // possible even when the installation command reports an error.
    store.set_setting("tls.ca_fingerprint", &fingerprint)?;
    if !system.trusted(&path) {
        store.conn.execute(
            "UPDATE certificates SET trusted=0 WHERE id='caddy-local'",
            [],
        )?;
        system.install(&path)?;
    }
    if !system.trusted(&path) {
        return fail("The CA is not present in the current user trust store after installation");
    }
    record_trusted(store, &path)
}
fn record_trusted(store: &Store, path: &std::path::Path) -> Result<()> {
    store.conn.execute("INSERT INTO certificates(id,path,trusted) VALUES('caddy-local',?1,1) ON CONFLICT(id) DO UPDATE SET path=excluded.path,trusted=1",[path.to_string_lossy().to_string()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    #[derive(Clone, Copy)]
    pub(super) enum Install {
        Success,
        Cancel,
        PartialFailure,
    }
    pub(super) struct FakeStore {
        pub(super) entries: RefCell<Vec<Vec<u8>>>,
        pub(super) install: Cell<Install>,
        pub(super) attempts: Cell<usize>,
        pub(super) removed: RefCell<Vec<Vec<u8>>>,
    }
    impl TrustStore for FakeStore {
        fn trusted(&self, path: &std::path::Path) -> bool {
            std::fs::read(path).is_ok_and(|der| self.entries.borrow().contains(&der))
        }
        fn install(&self, path: &std::path::Path) -> Result<()> {
            self.attempts.set(self.attempts.get() + 1);
            match self.install.get() {
                Install::Cancel => Err(crate::core::Error::Cancelled("cancelled".into())),
                Install::Success => {
                    self.entries.borrow_mut().push(std::fs::read(path)?);
                    Ok(())
                }
                Install::PartialFailure => {
                    self.entries.borrow_mut().push(std::fs::read(path)?);
                    fail("failed after mutation")
                }
            }
        }
        fn remove(&self, path: &std::path::Path) -> Result<()> {
            let der = std::fs::read(path)?;
            self.entries.borrow_mut().retain(|entry| *entry != der);
            self.removed.borrow_mut().push(der);
            Ok(())
        }
    }
    #[test]
    fn exact_ca_cancel_retry_partial_install_stale_records_and_foreign_subject_are_safe() {
        let d = tempfile::tempdir().unwrap();
        let h = Home::new(d.path());
        h.ensure().unwrap();
        let s = Store::open(&h.path("state.db")).unwrap();
        let path = CaddyTls.ca_path(&h);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let owned = b"same subject, exact owned DER";
        let unrelated = b"same subject, unrelated DER";
        std::fs::write(&path, owned).unwrap();
        let system = FakeStore {
            entries: RefCell::new(vec![unrelated.to_vec()]),
            install: Cell::new(Install::Cancel),
            attempts: Cell::new(0),
            removed: RefCell::new(vec![]),
        };
        assert!(matches!(
            trust_with(&s, &h, &system),
            Err(crate::core::Error::Cancelled(_))
        ));
        assert!(!system.trusted(&path));
        verify_owned(&s, &h).unwrap();
        assert_eq!(*system.entries.borrow(), vec![unrelated.to_vec()]);
        // DB ownership without a store entry: cleanup is a safe no-op.
        remove_with(&s, &h, &system).unwrap();
        assert!(system.removed.borrow().is_empty());
        system.install.set(Install::Success);
        trust_with(&s, &h, &system).unwrap();
        assert!(system.trusted(&path));
        assert_eq!(system.attempts.get(), 2);
        s.conn
            .execute("UPDATE certificates SET trusted=0", [])
            .unwrap();
        trust_with(&s, &h, &system).unwrap();
        assert_eq!(system.attempts.get(), 2);
        assert_eq!(
            s.conn
                .query_row(
                    "SELECT trusted FROM certificates WHERE id='caddy-local'",
                    [],
                    |r| r.get::<_, i32>(0)
                )
                .unwrap(),
            1
        );
        std::fs::write(&path, b"changed CA").unwrap();
        assert!(trust_with(&s, &h, &system).is_err());
        assert!(remove_with(&s, &h, &system).is_err());
        assert_eq!(system.entries.borrow().len(), 2);
        std::fs::write(&path, owned).unwrap();
        remove_with(&s, &h, &system).unwrap();
        assert_eq!(*system.entries.borrow(), vec![unrelated.to_vec()]);
        system.install.set(Install::PartialFailure);
        assert!(trust_with(&s, &h, &system).is_err());
        verify_owned(&s, &h).unwrap();
        assert!(system.trusted(&path));
        remove_with(&s, &h, &system).unwrap();
        assert_eq!(*system.entries.borrow(), vec![unrelated.to_vec()]);
        // Explicit trust can repair a stale DB; removal cannot adopt ownership.
        system.entries.borrow_mut().push(owned.to_vec());
        assert!(remove_with(&s, &h, &system).is_err());
        let attempts = system.attempts.get();
        trust_with(&s, &h, &system).unwrap();
        assert_eq!(system.attempts.get(), attempts);
        remove_with(&s, &h, &system).unwrap();
        assert_eq!(*system.entries.borrow(), vec![unrelated.to_vec()]);
    }
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
