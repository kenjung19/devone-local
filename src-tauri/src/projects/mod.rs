use crate::{
    core::{Result, fail, timestamp},
    storage::Store,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
pub trait ProjectAdapter: Send + Sync {
    fn kind(&self) -> &'static str;
    fn accepts(&self, project: &Path) -> bool;
    fn document_root(&self, project: &Path) -> PathBuf;
}
pub struct PlainPhp;
impl ProjectAdapter for PlainPhp {
    fn kind(&self) -> &'static str {
        "php"
    }
    fn accepts(&self, _: &Path) -> bool {
        true
    }
    fn document_root(&self, p: &Path) -> PathBuf {
        p.to_path_buf()
    }
}
pub struct Laravel;
impl ProjectAdapter for Laravel {
    fn kind(&self) -> &'static str {
        "laravel"
    }
    fn accepts(&self, p: &Path) -> bool {
        p.join("artisan").is_file() && p.join("public/index.php").is_file()
    }
    fn document_root(&self, p: &Path) -> PathBuf {
        p.join("public")
    }
}
pub fn detect(p: &Path) -> (&'static str, PathBuf) {
    let adapters: Vec<Box<dyn ProjectAdapter>> = vec![Box::new(Laravel), Box::new(PlainPhp)];
    let a = adapters
        .iter()
        .find(|a| a.accepts(p))
        .expect("fallback adapter");
    (a.kind(), a.document_root(p))
}
pub fn hostname(name: &str) -> Result<String> {
    let mut label = String::new();
    for ch in name.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            label.push(ch);
        } else if !label.ends_with('-') {
            label.push('-');
        }
    }
    let label = label.trim_matches('-');
    if label.is_empty() || label.len() > 63 {
        return fail("Name does not form a valid DNS label (1–63 ASCII characters)");
    }
    if ["localhost", "devone", "www", "test", "local", "admin"].contains(&label) {
        return fail(format!("Reserved hostname: {label}"));
    }
    Ok(format!("{label}.test"))
}
#[derive(serde::Serialize)]
pub struct Discovery {
    pub count: usize,
    pub issues: Vec<String>,
}
pub fn scan(store: &mut Store, www: &Path) -> Result<Discovery> {
    let mut entries = std::fs::read_dir(www)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    let mut detected = Vec::new();
    let mut issues = Vec::new();
    let mut hosts: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        // Symlinks/junctions are intentionally excluded to avoid escaping www.
        if !entry.file_type()?.is_dir() || crate::platform::is_link(&entry.path())? {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let (kind, root) = detect(&entry.path());
        let host = match hostname(&name) {
            Ok(h) => h,
            Err(e) => {
                issues.push(format!("{name}: {e}"));
                continue;
            }
        };
        *hosts.entry(host.clone()).or_default() += 1;
        detected.push((name, host, entry.path(), kind, root));
    }
    let tx = store.conn.transaction()?;
    tx.execute("UPDATE sites SET present=0", [])?;
    for (name, host, path, kind, root) in &detected {
        let path = path.to_string_lossy().to_string();
        let root = root.to_string_lossy().to_string();
        let existing: Option<String> = {
            use rusqlite::OptionalExtension;
            tx.query_row(
                "SELECT project_path FROM sites WHERE hostname=?1",
                [host],
                |r| r.get(0),
            )
            .optional()?
        };
        let conflict = hosts[host] > 1 || existing.as_ref().is_some_and(|p| p != &path);
        if conflict {
            issues.push(format!(
                "{name}: hostname conflict for {host}; rename the folder"
            ));
            tx.execute("UPDATE sites SET present=1,issue='Hostname conflict',updated_at=?2 WHERE project_path=?1", rusqlite::params![path,timestamp()])?;
            continue;
        }
        tx.execute("INSERT INTO sites(id,name,hostname,project_path,project_type,document_root,present,discovered_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,1,?7,?7) ON CONFLICT(project_path) DO UPDATE SET name=excluded.name,project_type=excluded.project_type,document_root=excluded.document_root,present=1,issue=NULL,updated_at=excluded.updated_at",
   rusqlite::params![uuid::Uuid::new_v4().to_string(),name,host,path,kind,root,timestamp()])?;
    }
    tx.commit()?;
    Ok(Discovery {
        count: detected.len(),
        issues,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostnames_are_safe_and_conflicts_visible() {
        assert_eq!(hostname("My App_1").unwrap(), "my-app-1.test");
        assert!(hostname("localhost").is_err());
        assert!(hostname("ภาษาไทย").is_err());
        assert!(hostname(&"a".repeat(64)).is_err());
        assert_eq!(hostname("foo_bar").unwrap(), hostname("foo bar").unwrap());
    }
    #[test]
    fn roots_and_discovery_preserve_removed_projects() {
        let d = tempfile::tempdir().unwrap();
        let www = d.path().join("www");
        std::fs::create_dir_all(www.join("app/public")).unwrap();
        std::fs::write(www.join("app/artisan"), "").unwrap();
        std::fs::write(www.join("app/public/index.php"), "").unwrap();
        std::fs::create_dir_all(www.join("plain/nested")).unwrap();
        assert_eq!(detect(&www.join("app")).0, "laravel");
        assert_eq!(detect(&www.join("plain")).1, www.join("plain"));
        let mut s = Store::open(&d.path().join("db")).unwrap();
        scan(&mut s, &www).unwrap();
        let n: i64 = s
            .conn
            .query_row("SELECT count(*) FROM sites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        std::fs::rename(www.join("app"), d.path().join("moved")).unwrap();
        scan(&mut s, &www).unwrap();
        let n: i64 = s
            .conn
            .query_row("SELECT count(*) FROM sites WHERE present=0", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
    }
    #[test]
    fn duplicate_hosts_do_not_choose_an_arbitrary_project() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("a b")).unwrap();
        std::fs::create_dir(d.path().join("a_b")).unwrap();
        let mut s = Store::open(&d.path().join("db")).unwrap();
        let result = scan(&mut s, d.path()).unwrap();
        assert_eq!(result.issues.len(), 2);
        let n: i64 = s
            .conn
            .query_row("SELECT count(*) FROM sites", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}
