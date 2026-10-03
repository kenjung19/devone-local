use crate::{
    config::Home,
    core::{Result, fail},
    storage::Store,
};
pub trait TlsProvider {
    fn ca_path(&self, home: &Home) -> std::path::PathBuf;
    fn trust(&self, store: &Store, home: &Home) -> Result<()>;
}
pub struct CaddyTls;
impl TlsProvider for CaddyTls {
    fn ca_path(&self, home: &Home) -> std::path::PathBuf {
        home.path("certs/caddy/pki/authorities/local/root.crt")
    }
    fn trust(&self, store: &Store, home: &Home) -> Result<()> {
        let path = self.ca_path(home);
        if !path.is_file() {
            return fail("Caddy CA is not generated yet. Start a configured site first.");
        }
        use sha2::{Digest, Sha256};
        let fingerprint = Sha256::digest(std::fs::read(&path)?)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if store.setting("tls.ca_fingerprint")?.as_deref() == Some(&fingerprint)
            && crate::platform::ca_trusted(&path)
        {
            return Ok(());
        }
        crate::platform::trust_ca(&path)?;
        if !crate::platform::ca_trusted(&path) {
            return fail(
                "The CA is not present in the current user trust store after installation",
            );
        }
        store.set_setting("tls.ca_fingerprint", &fingerprint)?;
        store.conn.execute("INSERT INTO certificates(id,path,trusted) VALUES('caddy-local',?1,1) ON CONFLICT(id) DO UPDATE SET path=excluded.path,trusted=1",[path.to_string_lossy().to_string()])?;
        Ok(())
    }
}
