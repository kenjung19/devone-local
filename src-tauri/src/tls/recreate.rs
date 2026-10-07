//! Persist intent before removing trust or moving key material. Interrupted
//! recreation restores the previous files; trust can then be explicitly retried.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Journal {
    backup: String,
    fingerprint: String,
    upgrade: String,
    legacy: String,
}
fn backup(home: &Home, journal: &Journal) -> Result<std::path::PathBuf> {
    uuid::Uuid::parse_str(&journal.backup)
        .map_err(|_| crate::core::Error::Message("Invalid CA recreate backup identifier".into()))?;
    let path = home
        .path("backups")
        .join(format!("caddy-ca-{}", journal.backup));
    authority::safe_path(home, &path)?;
    Ok(path)
}
pub fn resume_recreate(store: &Store, home: &Home) -> Result<()> {
    let Some(json) = store.setting("tls.ca_recreate")?.filter(|v| !v.is_empty()) else {
        return Ok(());
    };
    let journal: Journal = serde_json::from_str(&json)?;
    let base = backup(home, &journal)?;
    for (relative, name) in [("certs/devone-ca", "devone-ca"), ("certs/caddy", "caddy")] {
        let source = base.join(name);
        let target = home.path(relative);
        authority::safe_path(home, &source)?;
        authority::safe_path(home, &target)?;
        if source.exists() {
            if target.exists() {
                std::fs::rename(
                    &target,
                    base.join(format!("failed-{name}-{}", uuid::Uuid::new_v4())),
                )?;
            }
            std::fs::rename(source, target)?;
        }
    }
    store.set_setting("tls.ca_fingerprint", &journal.fingerprint)?;
    store.set_setting("tls.ca_upgrade", &journal.upgrade)?;
    store.set_setting("tls.legacy_ca_fingerprint", &journal.legacy)?;
    store.conn.execute(
        "UPDATE certificates SET trusted=0 WHERE id='caddy-local'",
        [],
    )?;
    store.set_setting("tls.ca_recreate", "")?;
    tracing::warn!(
        "Interrupted CA recreation restored previous files; verify HTTPS trust in Settings"
    );
    Ok(())
}
pub fn recover_ca(store: &Store, home: &Home, confirmed: bool) -> Result<()> {
    recreate_with(store, home, confirmed, true, &WindowsTrust, || {
        crate::setup::prepare_ca(store, home)
    })
}
pub(super) fn recreate_with(
    store: &Store,
    home: &Home,
    confirmed: bool,
    recover: bool,
    system: &impl TrustStore,
    prepare: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if !confirmed {
        return fail("Confirm CA recreation first");
    }
    if recover {
        if !upgrade::recovery_needed(store, home)? {
            return fail("CA upgrade is not in the blocked recovery state");
        }
    } else if upgrade_pending(store, home)? {
        return fail("Upgrade the legacy HTTPS certificate authority before recreating it");
    }
    resume_recreate(store, home)?;
    let journal = Journal {
        backup: uuid::Uuid::new_v4().to_string(),
        fingerprint: store.setting("tls.ca_fingerprint")?.unwrap_or_default(),
        upgrade: store.setting("tls.ca_upgrade")?.unwrap_or_default(),
        legacy: store
            .setting("tls.legacy_ca_fingerprint")?
            .unwrap_or_default(),
    };
    let base = backup(home, &journal)?;
    std::fs::create_dir(&base)?;
    crate::platform::harden_ca_path(&base)?;
    store.set_setting("tls.ca_recreate", &serde_json::to_string(&journal)?)?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
        if recover {
            // A missing archived certificate cannot be identified safely. Retain
            // that trust entry; never adopt a foreign current root by subject.
            for path in [
                CaddyTls.ca_path(home),
                authority::root_path(home),
                legacy_path(home),
                upgrade::archived_root(store, home)?.unwrap_or_default(),
            ] {
                if !path.is_file() {
                    continue;
                }
                authority::safe_path(home, &path)?;
                let actual = fingerprint(&path)?;
                if ![journal.fingerprint.as_str(), journal.legacy.as_str()]
                    .contains(&actual.as_str())
                {
                    continue;
                }
                if system.trusted(&path) {
                    system.remove(&path)?;
                    if system.trusted(&path) {
                        return fail("Owned CA trust remains present; recovery was stopped");
                    }
                }
            }
        } else {
            remove_with(store, home, system)?;
        }
        for (relative, name) in [("certs/devone-ca", "devone-ca"), ("certs/caddy", "caddy")] {
            let source = home.path(relative);
            authority::safe_path(home, &source)?;
            if source.exists() {
                std::fs::rename(source, base.join(name))?;
            }
        }
        store.set_setting("tls.ca_fingerprint", "")?;
        store.set_setting("tls.ca_upgrade", "")?;
        store.set_setting("tls.legacy_ca_fingerprint", "")?;
        prepare()?;
        authority::validate(home)?;
        store.set_setting("tls.ca_recreate", "")?;
        Ok(())
    }))
    .unwrap_or_else(|_| fail("CA recreation panicked; previous files will be restored"));
    if let Err(error) = result {
        if let Err(rollback) = resume_recreate(store, home) {
            return fail(format!("{error}; CA recreate rollback failed: {rollback}"));
        }
        return Err(error);
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
        let store = Store::open(&home.path("devone.db")).unwrap();
        authority::ensure(&home).unwrap();
        let bytes = std::fs::read(authority::root_path(&home)).unwrap();
        store
            .set_setting(
                "tls.ca_fingerprint",
                &fingerprint(&authority::root_path(&home)).unwrap(),
            )
            .unwrap();
        let system = FakeStore {
            entries: RefCell::new(vec![bytes, b"unrelated".to_vec()]),
            install: Cell::new(Install::Success),
            attempts: Cell::new(0),
            removed: RefCell::new(vec![]),
        };
        (d, home, store, system)
    }
    #[test]
    #[cfg(feature = "desktop")]
    fn recreate_cancel_restores_routes_and_records_error_without_moving_keys() {
        struct CancelTrust;
        impl TrustStore for CancelTrust {
            fn trusted(&self, _: &std::path::Path) -> bool {
                true
            }
            fn install(&self, _: &std::path::Path) -> Result<()> {
                unreachable!()
            }
            fn remove(&self, _: &std::path::Path) -> Result<()> {
                Err(crate::core::Error::Cancelled(
                    "simulated certutil cancel".into(),
                ))
            }
        }
        let (_d, home, store, _system) = fixture();
        let old = std::fs::read(authority::root_path(&home)).unwrap();
        let app = crate::app::Application::open_with_options(
            home.clone(),
            crate::app::Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        let shared = std::sync::Arc::new(std::sync::Mutex::new(app));
        assert!(
            crate::ipc::ca_operation(&shared, "recreation", || recreate_with(
                &store,
                &home,
                true,
                false,
                &CancelTrust,
                || panic!("cancel must prevent moves and prepare")
            ))
            .is_err()
        );
        assert!(!shared.lock().unwrap().ca_routes_blocked());
        assert!(
            shared
                .lock()
                .unwrap()
                .issues
                .iter()
                .any(|i| i.contains("simulated certutil cancel"))
        );
        assert_eq!(std::fs::read(authority::root_path(&home)).unwrap(), old);
        assert_eq!(
            store.setting("tls.ca_recreate").unwrap().as_deref(),
            Some("")
        );
    }
    #[test]
    #[cfg(feature = "desktop")]
    fn blocked_upgrade_recovery_preserves_data_and_removes_only_recorded_trust() {
        let (_d, home, store, system) = fixture();
        let old = fingerprint(&authority::root_path(&home)).unwrap();
        store.set_setting("tls.ca_upgrade", &serde_json::json!({"backup":uuid::Uuid::new_v4().to_string(),"legacy":"missing","owned_legacy":true,"replacement":old,"phase":"Switched"}).to_string()).unwrap();
        let pki = home.path("certs/caddy/pki");
        std::fs::create_dir_all(&pki).unwrap();
        std::fs::write(pki.join("evidence"), "preserve").unwrap();
        assert!(upgrade::recovery_needed(&store, &home).unwrap());
        assert!(recreate_with(&store, &home, false, true, &system, || Ok(())).is_err());
        let app = crate::app::Application::open_with_options(
            home.clone(),
            crate::app::Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        let shared = std::sync::Arc::new(std::sync::Mutex::new(app));
        assert!(shared.lock().unwrap().ca_routes_blocked());
        crate::ipc::ca_operation(&shared, "recovery", || {
            recreate_with(&store, &home, true, true, &system, || {
                authority::ensure(&home)
            })
        })
        .unwrap();
        assert!(!shared.lock().unwrap().ca_routes_blocked());
        assert!(
            !shared
                .lock()
                .unwrap()
                .issues
                .iter()
                .any(|i| i.starts_with("HTTPS CA "))
        );
        assert_eq!(
            store.setting("tls.ca_upgrade").unwrap().as_deref(),
            Some("")
        );
        assert_eq!(
            store
                .setting("tls.legacy_ca_fingerprint")
                .unwrap()
                .as_deref(),
            Some("")
        );
        assert_ne!(fingerprint(&authority::root_path(&home)).unwrap(), old);
        authority::validate(&home).unwrap();
        assert_eq!(*system.entries.borrow(), vec![b"unrelated".to_vec()]);
        assert_eq!(system.removed.borrow().len(), 1);
        assert!(
            std::fs::read_dir(home.path("backups")).unwrap().any(|e| e
                .unwrap()
                .path()
                .join("caddy/pki/evidence")
                .is_file())
        );
    }
    #[test]
    fn startup_restores_recreate_journal_after_move_without_deleting_partial_output() {
        let (_d, home, store, _system) = fixture();
        let old = std::fs::read(authority::root_path(&home)).unwrap();
        let journal = Journal {
            backup: uuid::Uuid::new_v4().to_string(),
            fingerprint: fingerprint(&authority::root_path(&home)).unwrap(),
            upgrade: String::new(),
            legacy: String::new(),
        };
        let base = backup(&home, &journal).unwrap();
        std::fs::create_dir(&base).unwrap();
        store
            .set_setting("tls.ca_recreate", &serde_json::to_string(&journal).unwrap())
            .unwrap();
        std::fs::rename(home.path("certs/devone-ca"), base.join("devone-ca")).unwrap();
        drop(store);
        let app = crate::app::Application::open_with_options(
            home.clone(),
            crate::app::Options {
                system_setup: false,
                autostart: false,
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(authority::root_path(&home)).unwrap(), old);
        assert_eq!(
            app.store.setting("tls.ca_recreate").unwrap().as_deref(),
            Some("")
        );
    }
}
