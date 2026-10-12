use crate::{
    config::Home,
    core::{Result, fail},
    storage::Store,
};
use std::path::{Path, PathBuf};

pub fn validate(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() || !path.is_dir() {
        return fail("Choose an existing absolute project folder");
    }
    let mut current = Some(path);
    while let Some(part) = current {
        if crate::platform::is_link(part)? {
            return fail("Project folders must not contain symlinks or junctions");
        }
        current = part.parent();
    }
    let canonical = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    let canonical = {
        let value = canonical.to_string_lossy();
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{unc}"))
        } else {
            PathBuf::from(value.strip_prefix(r"\\?\").unwrap_or(&value))
        }
    };
    Ok(canonical)
}

fn folded(path: &Path) -> PathBuf {
    // Windows paths compare case-insensitively.
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

/// Folders DEVONE may serve over HTTPS must not expose the system, a whole
/// drive or profile, or DEVONE's own Home (databases, credentials, CA key).
fn reject_unsafe(home: &Home, path: &Path) -> Result<()> {
    let candidate = folded(path);
    if path.parent().is_none() {
        return fail("Choose a project folder, not a whole drive or share");
    }
    for variable in ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
        if let Some(system) = std::env::var_os(variable)
            && candidate.starts_with(folded(Path::new(&system)))
        {
            return fail("Project folders cannot be inside Windows or program folders");
        }
    }
    if let Some(profile) = std::env::var_os("USERPROFILE")
        && candidate == folded(Path::new(&profile))
    {
        return fail("Choose a folder inside your user profile, not the whole profile");
    }
    let root = validate(home.root()).unwrap_or_else(|_| home.root().to_path_buf());
    let root = folded(&root);
    let www = root.join("www");
    if root.starts_with(&candidate)
        || (candidate.starts_with(&root) && !candidate.starts_with(&www))
    {
        return fail("Project folders cannot contain or be inside DEVONE Home data folders");
    }
    Ok(())
}

pub fn external(store: &Store) -> Result<Vec<PathBuf>> {
    Ok(store
        .setting("projects.external")?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default())
}

/// Applies the stored Projects folder. An invalid stored value must not stop
/// DEVONE from opening: fall back to Homewww and return the reason.
pub fn load_home(store: &Store, home: &mut Home) -> Result<Option<String>> {
    let Some(path) = store
        .setting("projects.root")?
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
    else {
        return Ok(None);
    };
    match validate(&path).and_then(|p| reject_unsafe(home, &p).map(|()| p)) {
        Ok(path) => {
            home.set_projects_root(Some(path));
            Ok(None)
        }
        Err(e) => {
            home.set_projects_root(None);
            Ok(Some(format!(
                "Projects folder {} is unavailable ({e}); using {} until it is changed in Settings",
                path.display(),
                home.www().display()
            )))
        }
    }
}

pub fn discovery_paths(store: &Store, www: &Path) -> Result<(Vec<PathBuf>, Vec<String>)> {
    let mut paths = Vec::new();
    let mut issues = Vec::new();
    match validate(www).and_then(|_| Ok(std::fs::read_dir(www)?)) {
        Ok(entries) => {
            // One unreadable folder must not hide every other project.
            for entry in entries {
                let checked = entry.map_err(Into::into).and_then(|entry| {
                    let path = entry.path();
                    if entry.file_type()?.is_dir() && !crate::platform::is_link(&path)? {
                        validate(&path).map(Some)
                    } else {
                        Ok(None)
                    }
                });
                match checked {
                    Ok(Some(path)) => paths.push(path),
                    Ok(None) => {}
                    Err(e) => issues.push(format!("Projects folder entry: {e}")),
                }
            }
        }
        Err(e) => issues.push(format!("Projects folder {}: {e}", www.display())),
    }
    for path in external(store)? {
        match validate(&path) {
            Ok(path) => paths.push(path),
            Err(e) => issues.push(format!("External project {}: {e}", path.display())),
        }
    }
    paths.sort();
    paths.dedup();
    Ok((paths, issues))
}

/// Moving the Projects folder keeps each project's identity. A site that lived
/// directly in the old folder is re-pointed to the same-named folder in the new
/// one, so its database binding, runtime choices, processes and settings (all
/// keyed by site id) follow it. Folders must be moved before the change.
fn carry_sites(store: &Store, old: &Path, new: &Path) -> Result<usize> {
    let old = folded(old);
    let tx = store.conn.unchecked_transaction()?;
    let rows = tx
        .prepare("SELECT id,project_path,document_root FROM sites")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut moved = 0;
    for (id, path, document_root) in rows {
        let path = PathBuf::from(path);
        let (Some(name), Some(parent)) = (path.file_name(), path.parent()) else {
            continue;
        };
        if folded(parent) != old {
            continue;
        }
        let Ok(target) = validate(&new.join(name)) else {
            continue;
        };
        let taken: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sites WHERE project_path=?1 COLLATE NOCASE AND id<>?2)",
            rusqlite::params![target.to_string_lossy(), id],
            |r| r.get(0),
        )?;
        if taken {
            continue;
        }
        let document = Path::new(&document_root)
            .strip_prefix(&path)
            .map(|relative| target.join(relative))
            .unwrap_or_else(|_| target.clone());
        tx.execute(
            "UPDATE sites SET project_path=?2,document_root=?3 WHERE id=?1",
            rusqlite::params![id, target.to_string_lossy(), document.to_string_lossy()],
        )?;
        moved += 1;
    }
    tx.commit()?;
    Ok(moved)
}

/// Returns how many existing projects kept their settings in the new folder.
pub fn set_root(store: &Store, home: &mut Home, path: Option<&Path>) -> Result<usize> {
    let path = path.map(validate).transpose()?;
    if let Some(path) = &path {
        reject_unsafe(home, path)?;
    }
    let old = validate(&home.www()).unwrap_or_else(|_| home.www());
    let new = match &path {
        Some(path) => path.clone(),
        None => validate(&home.path("www")).unwrap_or_else(|_| home.path("www")),
    };
    let moved = if folded(&old) == folded(&new) {
        0
    } else {
        carry_sites(store, &old, &new)?
    };
    store.set_setting(
        "projects.root",
        path.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .as_deref()
            .unwrap_or(""),
    )?;
    home.set_projects_root(path);
    Ok(moved)
}

pub fn add(store: &Store, home: &Home, path: &Path) -> Result<()> {
    let path = validate(path)?;
    reject_unsafe(home, &path)?;
    let host = super::hostname(&path.file_name().unwrap_or_default().to_string_lossy())?;
    // Adding a same-named folder would turn a working site into a conflict.
    let taken: bool = store.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sites WHERE hostname=?1 AND present=1 AND project_path<>?2 COLLATE NOCASE)",
        rusqlite::params![host, path.to_string_lossy()],
        |r| r.get(0),
    )?;
    if taken {
        return fail(format!(
            "{host} is already used by another project; rename one of the folders first"
        ));
    }
    let mut paths = external(store)?;
    if !paths.contains(&path) {
        paths.push(path);
        store.set_setting("projects.external", &serde_json::to_string(&paths)?)?;
    }
    Ok(())
}

pub fn remove(store: &Store, path: &Path) -> Result<()> {
    let mut paths = external(store)?;
    paths.retain(|p| p != path);
    store.set_setting("projects.external", &serde_json::to_string(&paths)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn root_and_external_projects_persist_without_moving_home_data() {
        let base = tempfile::tempdir().unwrap();
        let home = Home::new(base.path().join("home"));
        home.ensure().unwrap();
        let mut home = home;
        let root = base.path().join("projects");
        let other = base.path().join("elsewhere/shop");
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let mut store = Store::open(&home.path("devone.db")).unwrap();
        set_root(&store, &mut home, Some(&root)).unwrap();
        add(&store, &home, &other).unwrap();
        add(&store, &home, &other).unwrap();
        add(&store, &home, &root.join("app")).unwrap();
        super::super::scan(&mut store, &home.www()).unwrap();
        assert_eq!(
            store
                .conn
                .query_row("SELECT count(*) FROM sites WHERE present=1", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        let mut reopened = Home::new(home.root());
        assert!(load_home(&store, &mut reopened).unwrap().is_none());
        assert_eq!(reopened.www(), validate(&root).unwrap());
        assert!(home.path("runtimes").is_dir());
        assert!(!root.join("runtimes").exists());
        remove(&store, &validate(&other).unwrap()).unwrap();
        super::super::scan(&mut store, &home.www()).unwrap();
        assert!(other.is_dir());
        assert_eq!(
            store
                .conn
                .query_row("SELECT count(*) FROM sites WHERE present=1", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .conn
                .query_row("SELECT count(*) FROM sites WHERE present=0", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        set_root(&store, &mut home, None).unwrap();
        assert_eq!(home.www(), home.path("www"));
    }
    #[test]
    fn unsafe_roots_collisions_and_bad_stored_root_are_handled() {
        let base = tempfile::tempdir().unwrap();
        let mut home = Home::new(base.path().join("home"));
        home.ensure().unwrap();
        let store = Store::open(&home.path("devone.db")).unwrap();
        // Home itself, its data folders, a folder containing Home and a drive root.
        for path in [
            home.root().to_path_buf(),
            home.path("database"),
            base.path().to_path_buf(),
            validate(base.path())
                .unwrap()
                .ancestors()
                .last()
                .unwrap()
                .to_path_buf(),
        ] {
            assert!(
                set_root(&store, &mut home, Some(&path)).is_err(),
                "{}",
                path.display()
            );
            assert!(add(&store, &home, &path).is_err(), "{}", path.display());
        }
        // Projects inside Homewww stay allowed.
        std::fs::create_dir(home.path("www/inside")).unwrap();
        add(&store, &home, &home.path("www/inside")).unwrap();
        // A present site already owns shop.test: a second "shop" is refused.
        let first = base.path().join("a/shop");
        let second = base.path().join("b/shop");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let mut store = store;
        add(&store, &home, &first).unwrap();
        super::super::scan(&mut store, &home.www()).unwrap();
        assert!(
            add(&store, &home, &second)
                .unwrap_err()
                .to_string()
                .contains("shop.test")
        );
        // A stored root that is no longer valid falls back to Homewww.
        store.set_setting("projects.root", "relative").unwrap();
        let mut reopened = Home::new(home.root());
        let issue = load_home(&store, &mut reopened).unwrap().unwrap();
        assert!(issue.contains("Projects folder"));
        assert_eq!(reopened.www(), reopened.path("www"));
    }
    #[test]
    fn moving_projects_folder_keeps_site_identity_and_settings() {
        let base = tempfile::tempdir().unwrap();
        let mut home = Home::new(base.path().join("home"));
        home.ensure().unwrap();
        let mut store = Store::open(&home.path("devone.db")).unwrap();
        std::fs::create_dir_all(home.path("www/shop/public")).unwrap();
        std::fs::create_dir_all(home.path("www/left-behind")).unwrap();
        super::super::scan(&mut store, &home.www()).unwrap();
        let id_of = |store: &Store, host: &str| -> String {
            store
                .conn
                .query_row("SELECT id FROM sites WHERE hostname=?1", [host], |r| {
                    r.get(0)
                })
                .unwrap()
        };
        let shop = id_of(&store, "shop.test");
        store
            .conn
            .execute(
                "INSERT INTO site_runtime_overrides(site_id,kind,version) VALUES(?1,'php','8.3')",
                [&shop],
            )
            .unwrap();
        // The user moves only "shop", then points DEVONE at the new folder.
        let projects = base.path().join("Projects");
        std::fs::create_dir(&projects).unwrap();
        std::fs::rename(home.path("www/shop"), projects.join("shop")).unwrap();
        assert_eq!(set_root(&store, &mut home, Some(&projects)).unwrap(), 1);
        super::super::scan(&mut store, &home.www()).unwrap();
        assert_eq!(id_of(&store, "shop.test"), shop);
        let (path, present): (String, bool) = store
            .conn
            .query_row(
                "SELECT project_path,present FROM sites WHERE id=?1",
                [&shop],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(present);
        assert_eq!(
            PathBuf::from(path),
            validate(&projects.join("shop")).unwrap()
        );
        let version: String = store
            .conn
            .query_row(
                "SELECT version FROM site_runtime_overrides WHERE site_id=?1",
                [&shop],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, "8.3");
        // A project that was not moved is not re-pointed.
        let left: bool = store
            .conn
            .query_row(
                "SELECT present FROM sites WHERE hostname='left-behind.test'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!left);
    }
    #[test]
    fn missing_external_folder_is_reported_and_can_be_removed_without_deleting_files() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("www");
        let other = base.path().join("external");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&other).unwrap();
        let store = Store::open(&base.path().join("db")).unwrap();
        add(&store, &Home::new(base.path().join("home")), &other).unwrap();
        let other = validate(&other).unwrap();
        std::fs::remove_dir(&other).unwrap();
        let (paths, issues) = discovery_paths(&store, &root).unwrap();
        assert!(paths.is_empty());
        assert_eq!(issues.len(), 1);
        remove(&store, &other).unwrap();
        assert!(external(&store).unwrap().is_empty());
        assert!(validate(Path::new("relative")).is_err());
        assert!(validate(&base.path().join("missing")).is_err());
    }
}
