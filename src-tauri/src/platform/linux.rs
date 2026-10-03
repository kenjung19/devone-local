use super::*;
pub fn default_home() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
        .ok_or_else(|| crate::core::Error::Message("No user data directory".into()))?;
    Ok(base.join("devone"))
}
pub fn is_link(p: &Path) -> Result<bool> {
    Ok(std::fs::symlink_metadata(p)?.file_type().is_symlink())
}
pub fn configure(_: &mut Command) {}
pub fn open(_: &str) -> Result<()> {
    fail("Linux desktop support is planned")
}
pub fn terminal(_: &Path, _: &BTreeMap<String, String>) -> Result<()> {
    fail("Linux terminal support is planned")
}
pub fn sync_hosts(_: &[String]) -> Result<()> {
    fail("Linux DNS support is planned")
}
pub fn hosts_ready(_: &[String]) -> bool {
    false
}
pub fn trust_ca(_: &Path) -> Result<()> {
    fail("Linux trust support is planned")
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
