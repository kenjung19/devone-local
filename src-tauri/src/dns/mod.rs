use crate::core::{Result, fail};
pub mod server;
pub trait DnsProvider {
    fn reconcile(&self, hosts: &[String]) -> Result<()>;
    fn ready(&self, hosts: &[String]) -> bool;
}
pub struct LocalDns;
impl DnsProvider for LocalDns {
    fn reconcile(&self, _hosts: &[String]) -> Result<()> {
        if crate::platform::wildcard_ready() {
            Ok(())
        } else {
            fail(
                "Wildcard DNS setup required. Open Setup to install the .test-only Windows policy.",
            )
        }
    }
    fn ready(&self, _hosts: &[String]) -> bool {
        crate::platform::wildcard_system_ready()
    }
}
pub struct HostsFallback;
impl DnsProvider for HostsFallback {
    fn reconcile(&self, hosts: &[String]) -> Result<()> {
        crate::platform::sync_hosts(hosts)
    }
    fn ready(&self, hosts: &[String]) -> bool {
        crate::platform::hosts_ready(hosts)
    }
}
