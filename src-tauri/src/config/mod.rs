use crate::core::Result;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone)]
pub struct Home {
    root: PathBuf,
}
impl Home {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn resolve() -> Result<Self> {
        let root = match std::env::var_os("DEVONE_HOME") {
            Some(v) => PathBuf::from(v),
            None => crate::platform::default_home()?,
        };
        if !root.is_absolute() {
            return crate::core::fail("DEVONE_HOME must be an absolute path");
        }
        Ok(Self::new(root))
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    pub fn www(&self) -> PathBuf {
        self.path("www")
    }
    pub fn runtime(&self, kind: &str, version: &str) -> PathBuf {
        self.path("runtimes").join(kind).join(version)
    }
    pub fn ensure(&self) -> Result<()> {
        for dir in [
            "www", "runtimes", "tools", "database", "config", "certs", "logs", "backups", "cache",
        ] {
            std::fs::create_dir_all(self.path(dir))?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_are_relative_to_one_provider() {
        let h = Home::new(PathBuf::from("fixture"));
        assert_eq!(
            h.runtime("php", "8.3"),
            PathBuf::from("fixture").join("runtimes/php/8.3")
        );
    }
}
