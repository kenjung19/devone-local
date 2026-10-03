//! Authoritative loopback resolver. Only .test is answered; unrelated traffic is refused.
use crate::core::{Result, fail};
use std::{
    io::{Read, Write},
    net::{TcpListener, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
pub struct Resolver {
    stop: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
    pub port: u16,
}
fn response(query: &[u8]) -> Option<Vec<u8>> {
    if query.len() < 12 || query[2] & 0x80 != 0 || query[4..6] != [0, 1] {
        return None;
    }
    let mut p = 12;
    let mut labels = Vec::new();
    loop {
        let n = *query.get(p)? as usize;
        p += 1;
        if n == 0 {
            break;
        }
        if n > 63 || p + n > query.len() {
            return None;
        }
        labels.push(
            std::str::from_utf8(&query[p..p + n])
                .ok()?
                .to_ascii_lowercase(),
        );
        p += n;
        if p > 267 {
            return None;
        }
    }
    let qtype = u16::from_be_bytes([*query.get(p)?, *query.get(p + 1)?]);
    let class = u16::from_be_bytes([*query.get(p + 2)?, *query.get(p + 3)?]);
    p += 4;
    let managed = labels.len() > 1 && labels.last().is_some_and(|s| s == "test") && class == 1;
    let answer = managed && (qtype == 1 || qtype == 255);
    let mut out = query[..p].to_vec();
    out[2] = 0x84 | (query[2] & 1);
    out[3] = if managed { 0 } else { 5 };
    out[6..12].fill(0);
    if answer {
        out[7] = 1;
        out.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 5, 0, 4, 127, 0, 0, 1]);
    }
    Some(out)
}
impl Resolver {
    pub fn start(port: u16) -> Result<Self> {
        let udp = UdpSocket::bind(("127.0.0.1", port))
            .map_err(|e| crate::core::Error::Message(format!("DNS UDP {port} conflict: {e}")))?;
        let port = udp.local_addr()?.port();
        let tcp = TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| crate::core::Error::Message(format!("DNS TCP {port} conflict: {e}")))?;
        udp.set_read_timeout(Some(Duration::from_millis(100)))?;
        tcp.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let a = stop.clone();
        let b = stop.clone();
        let workers = vec![
            thread::spawn(move || {
                let mut buf = [0; 4096];
                while !a.load(Ordering::Relaxed) {
                    if let Ok((n, peer)) = udp.recv_from(&mut buf)
                        && let Some(out) = response(&buf[..n])
                    {
                        let _ = udp.send_to(&out, peer);
                    }
                }
            }),
            thread::spawn(move || {
                while !b.load(Ordering::Relaxed) {
                    match tcp.accept() {
                        Ok((mut s, _)) => {
                            let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
                            let _ = s.set_write_timeout(Some(Duration::from_millis(200)));
                            let mut len = [0; 2];
                            if s.read_exact(&mut len).is_ok() {
                                let n = u16::from_be_bytes(len) as usize;
                                if n <= 4096 {
                                    let mut q = vec![0; n];
                                    if s.read_exact(&mut q).is_ok()
                                        && let Some(out) = response(&q)
                                    {
                                        let _ = s.write_all(&(out.len() as u16).to_be_bytes());
                                        let _ = s.write_all(&out);
                                    }
                                }
                            }
                        }
                        Err(_) => thread::sleep(Duration::from_millis(25)),
                    }
                }
            }),
        ];
        tracing::info!(port, "wildcard DNS started");
        Ok(Self {
            stop,
            workers,
            port,
        })
    }
    pub fn healthy(&self) -> bool {
        probe(self.port).is_ok()
    }
}
pub fn probe(port: u16) -> Result<()> {
    let mut q = vec![0x44, 0x31, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    q.extend_from_slice(&[
        6, b'd', b'e', b'v', b'o', b'n', b'e', 4, b't', b'e', b's', b't', 0, 0, 1, 0, 1,
    ]);
    let socket = UdpSocket::bind(("127.0.0.1", 0))?;
    socket.set_read_timeout(Some(Duration::from_millis(300)))?;
    socket.connect(("127.0.0.1", port))?;
    socket.send(&q)?;
    let mut out = [0; 512];
    let n = socket.recv(&mut out)?;
    if n != q.len() + 16
        || out[..2] != q[..2]
        || out[3] & 15 != 0
        || out[n - 4..n] != [127, 0, 0, 1]
    {
        return fail("DNS health response is invalid");
    }
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut tcp = std::net::TcpStream::connect_timeout(&address, Duration::from_millis(300))?;
    tcp.set_read_timeout(Some(Duration::from_millis(300)))?;
    tcp.set_write_timeout(Some(Duration::from_millis(300)))?;
    tcp.write_all(&(q.len() as u16).to_be_bytes())?;
    tcp.write_all(&q)?;
    let mut len = [0; 2];
    tcp.read_exact(&mut len)?;
    let length = u16::from_be_bytes(len) as usize;
    if length != q.len() + 16 {
        return fail("DNS TCP health response length is invalid");
    }
    let mut reply = vec![0; length];
    tcp.read_exact(&mut reply)?;
    if reply[..2] != q[..2] || reply[length - 4..] != [127, 0, 0, 1] {
        return fail("DNS TCP health response is invalid");
    }
    Ok(())
}
impl Drop for Resolver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
        tracing::info!("wildcard DNS stopped");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn query(name: &str, t: u16) -> Vec<u8> {
        let mut q = vec![1, 2, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        for l in name.split('.') {
            q.push(l.len() as u8);
            q.extend_from_slice(l.as_bytes());
        }
        q.push(0);
        q.extend_from_slice(&t.to_be_bytes());
        q.extend_from_slice(&[0, 1]);
        q
    }
    #[test]
    fn wildcard_and_unrelated_names() {
        let a = response(&query("new.project.test", 1)).unwrap();
        assert_eq!(&a[a.len() - 4..], &[127, 0, 0, 1]);
        assert_eq!(response(&query("google.com", 1)).unwrap()[3], 5);
        assert_eq!(response(&query("new.test", 28)).unwrap()[7], 0);
        assert!(response(&[0; 4]).is_none());
    }
    #[test]
    fn udp_tcp_lifecycle_and_conflicts() {
        let r = Resolver::start(0).unwrap();
        assert!(r.healthy());
        assert!(Resolver::start(r.port).is_err());
        let mut s = std::net::TcpStream::connect(("127.0.0.1", r.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let q = query("future.test", 1);
        s.write_all(&(q.len() as u16).to_be_bytes()).unwrap();
        s.write_all(&q).unwrap();
        let mut len = [0; 2];
        s.read_exact(&mut len).unwrap();
        let mut out = vec![0; u16::from_be_bytes(len) as usize];
        s.read_exact(&mut out).unwrap();
        assert_eq!(out[out.len() - 4..], [127, 0, 0, 1]);
        let p = r.port;
        drop(r);
        assert!(Resolver::start(p).is_ok());
    }
}
