//! Explicit, resumable replacement. The legacy root stays trusted until the
//! replacement is confirmed trusted. Cancel rolls the served chain back.
use super::{TrustStore, WindowsTrust, authority, fingerprint, legacy_path, record_trusted};
use crate::{
    config::Home,
    core::{Error, Result, fail},
    storage::Store,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub enum Reload {
    Stop,
    Start,
}
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
enum Phase {
    Prepared,
    Switched,
    Trusted,
    Removed,
    Complete,
}
#[derive(Serialize, Deserialize)]
struct Upgrade {
    backup: String,
    legacy: String,
    owned_legacy: bool,
    replacement: String,
    phase: Phase,
}
fn load(store: &Store) -> Result<Option<Upgrade>> {
    store
        .setting("tls.ca_upgrade")?
        .filter(|s| !s.is_empty())
        .map(|s| Ok(serde_json::from_str(&s)?))
        .transpose()
}
fn save(store: &Store, state: &Upgrade) -> Result<()> {
    store.set_setting("tls.ca_upgrade", &serde_json::to_string(state)?)
}
fn backup(home: &Home, state: &Upgrade) -> Result<PathBuf> {
    // The persisted value is an identifier, never an arbitrary move destination.
    uuid::Uuid::parse_str(&state.backup)
        .map_err(|_| Error::Message("Invalid CA upgrade backup identifier".into()))?;
    let path = home
        .path("backups")
        .join(format!("legacy-ca-{}", state.backup));
    authority::safe_path(home, &path)?;
    Ok(path)
}
pub fn upgrade_pending(store: &Store, home: &Home) -> Result<bool> {
    if let Some(state) = load(store)? {
        return Ok(state.phase != Phase::Complete);
    }
    Ok(legacy_path(home).exists())
}
pub fn upgrade(store: &Store, home: &Home, reload: impl FnMut(Reload) -> Result<()>) -> Result<()> {
    upgrade_with(store, home, &WindowsTrust, reload)
}
fn verify_legacy(store: &Store, home: &Home, state: &Upgrade) -> Result<PathBuf> {
    let archived = backup(home, state)?.join("root.crt");
    authority::safe_path(home, &archived)?;
    if fingerprint(&archived)? != state.legacy {
        return fail("Legacy CA backup fingerprint mismatch; no certificate was removed");
    }
    if state.owned_legacy
        && state.phase != Phase::Removed
        && store.setting("tls.legacy_ca_fingerprint")?.as_deref() != Some(&state.legacy)
    {
        return fail("Legacy CA ownership fingerprint mismatch; no certificate was removed");
    }
    let original = legacy_path(home);
    if state.phase == Phase::Prepared
        && original.is_file()
        && fingerprint(&original)? != state.legacy
    {
        return fail("Legacy CA identity changed; upgrade refused without removing trust");
    }
    Ok(archived)
}
fn archive(home: &Home, state: &Upgrade) -> Result<()> {
    let base = backup(home, state)?;
    for (source, name) in [
        ("certs/caddy/pki", "pki"),
        ("certs/caddy/certificates/local", "certificates-local"),
    ] {
        let source = home.path(source);
        let destination = base.join(name);
        authority::safe_path(home, &source)?;
        authority::safe_path(home, &destination)?;
        if !destination.exists() && source.exists() {
            std::fs::rename(source, destination)?;
        }
    }
    Ok(())
}
fn rollback(store: &Store, home: &Home, state: &mut Upgrade) -> Result<()> {
    let base = backup(home, state)?;
    for (destination, name) in [
        ("certs/caddy/pki", "pki"),
        ("certs/caddy/certificates/local", "certificates-local"),
    ] {
        let destination = home.path(destination);
        let source = base.join(name);
        authority::safe_path(home, &destination)?;
        authority::safe_path(home, &source)?;
        if source.exists() {
            if destination.exists() {
                std::fs::rename(
                    &destination,
                    base.join(format!("new-{name}-{}", uuid::Uuid::new_v4())),
                )?;
            }
            std::fs::create_dir_all(destination.parent().unwrap())?;
            std::fs::rename(source, destination)?;
        }
    }
    let active = home.path("certs/devone-ca/active");
    authority::safe_path(home, &active)?;
    if active.exists() {
        std::fs::remove_file(active)?;
    }
    store.set_setting(
        "tls.ca_fingerprint",
        if state.owned_legacy {
            &state.legacy
        } else {
            ""
        },
    )?;
    store.conn.execute(
        "UPDATE certificates SET path=?1 WHERE id='caddy-local'",
        [legacy_path(home).to_string_lossy().to_string()],
    )?;
    state.phase = Phase::Prepared;
    save(store, state)
}
pub fn resume_safe_pending(store: &Store, home: &Home) -> Result<()> {
    resume_with(store, home, &WindowsTrust)
}
fn resume_with(store: &Store, home: &Home, system: &impl TrustStore) -> Result<()> {
    if let Some(mut state) = load(store)?
        && state.phase == Phase::Switched
        && !system.trusted(&authority::root_path(home))
    {
        verify_legacy(store, home, &state)?;
        rollback(store, home, &mut state)?;
    }
    Ok(())
}
fn upgrade_with(
    store: &Store,
    home: &Home,
    system: &impl TrustStore,
    mut reload: impl FnMut(Reload) -> Result<()>,
) -> Result<()> {
    let mut state = if let Some(state) = load(store)? {
        if state.phase == Phase::Complete {
            return Ok(());
        }
        state
    } else {
        let legacy = legacy_path(home);
        if !legacy.exists() {
            authority::ensure(home)?;
            return super::trust_with(store, home, system);
        }
        authority::safe_path(home, &legacy)?;
        let actual = fingerprint(&legacy)?;
        let recorded = store.setting("tls.ca_fingerprint")?.unwrap_or_default();
        if (!recorded.is_empty() && recorded != actual)
            || (recorded.is_empty() && system.trusted(&legacy))
        {
            return fail("Legacy CA ownership mismatch; upgrade refused without changing trust");
        }
        authority::ensure(home)?;
        let state = Upgrade {
            backup: uuid::Uuid::new_v4().to_string(),
            legacy: actual,
            owned_legacy: !recorded.is_empty(),
            replacement: fingerprint(&authority::root_path(home))?,
            phase: Phase::Prepared,
        };
        let base = backup(home, &state)?;
        std::fs::create_dir(&base)?;
        crate::platform::harden_ca_path(&base)?;
        crate::runtime::atomic_write(&base.join("root.crt"), &std::fs::read(&legacy)?)?;
        // Persist old ownership separately before changing the active fingerprint.
        store.set_setting(
            "tls.legacy_ca_fingerprint",
            if state.owned_legacy {
                &state.legacy
            } else {
                ""
            },
        )?;
        save(store, &state)?;
        state
    };
    let old_cert = verify_legacy(store, home, &state)?;
    authority::validate(home)?;
    let new_cert = authority::root_path(home);
    if fingerprint(&new_cert)? != state.replacement {
        return fail("Replacement CA identity changed; upgrade refused without removing trust");
    }
    if state.phase == Phase::Prepared {
        reload(Reload::Stop)?;
        // Write the intent before moves: interrupted partial switches are rolled
        // back on next launch if the replacement was not yet trusted.
        state.phase = Phase::Switched;
        save(store, &state)?;
    }
    if state.phase == Phase::Switched {
        let switching = (|| -> Result<()> {
            reload(Reload::Stop)?;
            archive(home, &state)?;
            let active = home.path("certs/devone-ca/active");
            authority::safe_path(home, &active)?;
            crate::runtime::atomic_write(&active, state.replacement.as_bytes())?;
            reload(Reload::Start)?;
            // Record replacement ownership before the OS operation.
            store.set_setting("tls.ca_fingerprint", &state.replacement)?;
            if !system.trusted(&new_cert) {
                system.install(&new_cert)?;
            }
            if !system.trusted(&new_cert) {
                return fail("Replacement CA trust is not confirmed; legacy trust was retained");
            }
            record_trusted(store, &new_cert)
        })();
        if let Err(error) = switching {
            if !system.trusted(&new_cert) {
                reload(Reload::Stop)?;
                rollback(store, home, &mut state)?;
                reload(Reload::Start)?;
            }
            return Err(error);
        }
        state.phase = Phase::Trusted;
        save(store, &state)?;
    }
    if state.phase == Phase::Trusted {
        if !system.trusted(&new_cert) {
            store.set_setting("tls.ca_fingerprint", &state.replacement)?;
            system.install(&new_cert)?;
            if !system.trusted(&new_cert) {
                return fail("New CA trust is not confirmed; legacy trust retained, retry upgrade");
            }
        }
        if system.trusted(&old_cert) {
            if !state.owned_legacy {
                return fail("Legacy root is trusted but not owned; nothing removed");
            }
            verify_legacy(store, home, &state)?;
            system.remove(&old_cert)?;
            if system.trusted(&old_cert) {
                return fail("Legacy trust is still present; retry upgrade");
            }
        }
        state.phase = Phase::Removed;
        save(store, &state)?;
    }
    if state.phase == Phase::Removed {
        store.set_setting("tls.legacy_ca_fingerprint", "")?;
        state.phase = Phase::Complete;
        save(store, &state)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::tests::{FakeStore, Install};
    use std::cell::{Cell, RefCell};
    fn fixture() -> (tempfile::TempDir, Home, Store, FakeStore) {
        let d = tempfile::tempdir().unwrap();
        let home = Home::new(d.path());
        home.ensure().unwrap();
        let store = Store::open(&home.path("state.db")).unwrap();
        let legacy = legacy_path(&home);
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"legacy exact CA").unwrap();
        store
            .set_setting("tls.ca_fingerprint", &fingerprint(&legacy).unwrap())
            .unwrap();
        let system = FakeStore {
            entries: RefCell::new(vec![b"legacy exact CA".to_vec()]),
            install: Cell::new(Install::Success),
            attempts: Cell::new(0),
            removed: RefCell::new(vec![]),
        };
        (d, home, store, system)
    }
    #[test]
    fn legacy_upgrade_cancel_retry_and_crash_after_new_trust_are_resumable() {
        let (_d, home, store, system) = fixture();
        system.install.set(Install::Cancel);
        assert!(upgrade_with(&store, &home, &system, |_| Ok(())).is_err());
        assert!(system.trusted(&legacy_path(&home)));
        assert_eq!(super::super::CaddyTls.ca_path(&home), legacy_path(&home));
        assert!(upgrade_pending(&store, &home).unwrap());
        assert!(system.removed.borrow().is_empty());
        system.install.set(Install::PartialFailure);
        assert!(upgrade_with(&store, &home, &system, |_| Ok(())).is_err());
        assert!(system.trusted(&authority::root_path(&home)));
        assert!(system.removed.borrow().is_empty());
        upgrade_with(&store, &home, &system, |_| Ok(())).unwrap();
        assert!(!upgrade_pending(&store, &home).unwrap());
        assert_eq!(*system.removed.borrow(), [b"legacy exact CA".to_vec()]);
        assert!(system.trusted(&authority::root_path(&home)));
        assert!(
            backup(&home, &load(&store).unwrap().unwrap())
                .unwrap()
                .join("pki")
                .is_dir()
        );
    }
    #[test]
    fn fingerprint_mismatch_does_not_remove_any_certificate() {
        let (_d, home, store, system) = fixture();
        store.set_setting("tls.ca_fingerprint", "foreign").unwrap();
        assert!(upgrade_with(&store, &home, &system, |_| Ok(())).is_err());
        assert!(system.removed.borrow().is_empty());
        assert!(system.trusted(&legacy_path(&home)));
        assert!(!authority::root_path(&home).exists());
    }
    #[test]
    fn fresh_install_has_no_legacy_removal() {
        let (_d, home, store, system) = fixture();
        std::fs::remove_file(legacy_path(&home)).unwrap();
        store.set_setting("tls.ca_fingerprint", "").unwrap();
        upgrade_with(&store, &home, &system, |_| Ok(())).unwrap();
        assert!(system.trusted(&authority::root_path(&home)));
        assert!(system.removed.borrow().is_empty());
    }
    #[test]
    fn legacy_happy_path_rebuilds_cache_and_removes_only_recorded_root() {
        let (_d, home, store, system) = fixture();
        system.entries.borrow_mut().push(b"unrelated root".to_vec());
        let leaves = home.path("certs/caddy/certificates/local");
        std::fs::create_dir_all(&leaves).unwrap();
        std::fs::write(leaves.join("legacy-leaf"), b"old leaf").unwrap();
        upgrade_with(&store, &home, &system, |mode| {
            if matches!(mode, Reload::Start) {
                assert_eq!(
                    super::super::CaddyTls.ca_path(&home),
                    authority::root_path(&home)
                );
                assert!(!legacy_path(&home).exists());
                assert!(!leaves.join("legacy-leaf").exists());
                std::fs::create_dir_all(&leaves)?;
                std::fs::write(leaves.join("new-leaf"), b"new leaf")?;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(*system.removed.borrow(), [b"legacy exact CA".to_vec()]);
        assert!(
            system
                .entries
                .borrow()
                .contains(&b"unrelated root".to_vec())
        );
        assert_eq!(
            store.setting("tls.ca_fingerprint").unwrap().unwrap(),
            fingerprint(&authority::root_path(&home)).unwrap()
        );
        let state = load(&store).unwrap().unwrap();
        assert_eq!(
            std::fs::read(
                backup(&home, &state)
                    .unwrap()
                    .join("certificates-local/legacy-leaf")
            )
            .unwrap(),
            b"old leaf"
        );
        upgrade_with(&store, &home, &system, |_| {
            panic!("completed upgrade must not reload")
        })
        .unwrap();
    }
    #[test]
    fn interrupted_switch_restores_legacy_before_startup_and_can_resume() {
        let (_d, home, store, system) = fixture();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            upgrade_with(&store, &home, &system, |mode| {
                if matches!(mode, Reload::Start) {
                    panic!("crash before Windows trust");
                }
                Ok(())
            })
            .unwrap();
        }));
        assert!(result.is_err());
        assert!(!legacy_path(&home).exists());
        resume_with(&store, &home, &system).unwrap();
        assert_eq!(super::super::CaddyTls.ca_path(&home), legacy_path(&home));
        assert!(system.trusted(&legacy_path(&home)));
        assert!(upgrade_pending(&store, &home).unwrap());
        assert!(system.removed.borrow().is_empty());
        upgrade_with(&store, &home, &system, |_| Ok(())).unwrap();
        assert!(!upgrade_pending(&store, &home).unwrap());
    }
    #[test]
    fn persisted_legacy_ownership_mismatch_after_trust_never_removes_it() {
        let (_d, home, store, system) = fixture();
        system.install.set(Install::PartialFailure);
        assert!(upgrade_with(&store, &home, &system, |_| Ok(())).is_err());
        store
            .set_setting("tls.legacy_ca_fingerprint", "mismatch")
            .unwrap();
        assert!(upgrade_with(&store, &home, &system, |_| Ok(())).is_err());
        assert!(system.removed.borrow().is_empty());
        assert!(
            system
                .entries
                .borrow()
                .contains(&b"legacy exact CA".to_vec())
        );
    }
    use super::super::TlsProvider;
}
