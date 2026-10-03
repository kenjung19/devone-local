use super::*;
pub fn default_home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("HOME")
            .ok_or_else(|| crate::core::Error::Message("No user home".into()))?,
    )
    .join("Library/Application Support/Devone"))
}
pub fn is_link(p: &Path) -> Result<bool> {
    Ok(std::fs::symlink_metadata(p)?.file_type().is_symlink())
}
pub fn configure(_: &mut Command) {}
pub fn open(_: &str) -> Result<()> {
    fail("macOS desktop support is planned")
}
pub fn terminal(_: &Path, _: &BTreeMap<String, String>) -> Result<()> {
    fail("macOS terminal support is planned")
}
pub fn sync_hosts(_: &[String]) -> Result<()> {
    fail("macOS DNS support is planned")
}
pub fn hosts_ready(_: &[String]) -> bool {
    false
}
pub fn trust_ca(_: &Path) -> Result<()> {
    fail("macOS trust support is planned")
}
pub struct Ownership;
impl Ownership {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
    pub fn attach(&self, _: &Child) -> Result<()> {
        Ok(())
    }
}
