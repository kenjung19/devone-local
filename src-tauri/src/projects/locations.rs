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

pub fn external(store: &Store) -> Result<Vec<PathBuf>> {
    Ok(store
        .setting("projects.external")?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default())
}

pub fn load_home(store: &Store, home: &mut Home) -> Result<()> {
    let path = store
        .setting("projects.root")?
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    if path.as_ref().is_some_and(|p| !p.is_absolute()) {
        return fail("Configured Projects folder must be absolute");
    }
    home.set_projects_root(path);
    Ok(())
}

pub fn discovery_paths(store: &Store, www: &Path) -> Result<(Vec<PathBuf>, Vec<String>)> {
    let mut paths = Vec::new();
    let mut issues = Vec::new();
    match validate(www).and_then(|_| Ok(std::fs::read_dir(www)?)) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_dir() && !crate::platform::is_link(&entry.path())? {
                    paths.push(validate(&entry.path())?);
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

pub fn set_root(store: &Store, home: &mut Home, path: Option<&Path>) -> Result<()> {
    let path = path.map(validate).transpose()?;
    store.set_setting(
        "projects.root",
        path.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .as_deref()
            .unwrap_or(""),
    )?;
    home.set_projects_root(path);
    Ok(())
}

pub fn add(store: &Store, path: &Path) -> Result<()> {
    let path = validate(path)?;
    super::hostname(&path.file_name().unwrap_or_default().to_string_lossy())?;
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
        add(&store, &other).unwrap();
        add(&store, &other).unwrap();
        add(&store, &root.join("app")).unwrap();
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
        load_home(&store, &mut reopened).unwrap();
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
    fn missing_external_folder_is_reported_and_can_be_removed_without_deleting_files() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("www");
        let other = base.path().join("external");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&other).unwrap();
        let store = Store::open(&base.path().join("db")).unwrap();
        add(&store, &other).unwrap();
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
