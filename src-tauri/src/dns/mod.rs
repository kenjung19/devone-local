use crate::core::Result;
pub trait DnsProvider {
    fn reconcile(&self, hosts: &[String]) -> Result<()>;
    fn ready(&self, hosts: &[String]) -> bool;
}
pub struct LocalDns;
impl DnsProvider for LocalDns {
    fn reconcile(&self, hosts: &[String]) -> Result<()> {
        crate::platform::sync_hosts(hosts)
    }
    fn ready(&self, hosts: &[String]) -> bool {
        crate::platform::hosts_ready(hosts)
    }
}
