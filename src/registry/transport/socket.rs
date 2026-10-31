use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

// A timed-out OS DNS lookup cannot be cancelled portably. Its slot remains held
// until the resolver actually exits, so repeated timeouts cannot spawn an
// unbounded collection of stranded workers. No application lookup queue exists.
static RESOLVERS: AtomicUsize = AtomicUsize::new(0);
const MAX_ADDRESSES: usize = 32;

struct ResolverSlot;
impl Drop for ResolverSlot {
    fn drop(&mut self) {
        RESOLVERS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn resolve(host: &str, port: u16, end: Instant) -> Result<Vec<SocketAddr>, Error> {
    remaining(end)?;
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        return Ok(vec![SocketAddr::from((ip, port))]);
    }
    RESOLVERS
        .try_update(Ordering::AcqRel, Ordering::Acquire, |active| {
            (active < MAX_CONNECTIONS).then_some(active + 1)
        })
        .map_err(|_| error(ErrorKind::Busy))?;
    let slot = ResolverSlot;
    let host = host.to_owned();
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("registry-dns".into())
        .spawn(move || {
            let _slot = slot;
            let result = (host.as_str(), port).to_socket_addrs().map(|addresses| {
                // The origin host may resolve to IPv6 even though literal IPv6
                // origins are not part of profile 1. TLS still verifies that host.
                addresses.take(MAX_ADDRESSES).collect::<Vec<_>>()
            });
            let _ = sender.send(result);
        })
        .map_err(Error::from)?;
    receiver
        .recv_timeout(remaining(end)?)
        .map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => error(ErrorKind::Timeout),
            mpsc::RecvTimeoutError::Disconnected => error(ErrorKind::Transport),
        })?
        .map_err(Error::from)
}

/// Every low-level TLS read/write uses the remaining absolute budget. A peer
/// that drips bytes cannot renew the deadline by making partial progress.
pub(super) struct Socket {
    pub(super) stream: TcpStream,
    pub(super) end: Instant,
}
impl Socket {
    pub(super) fn connect(host: &str, port: u16, end: Instant) -> Result<Self, Error> {
        let addresses = resolve(host, port, end)?;
        for address in addresses {
            if let Ok(stream) = TcpStream::connect_timeout(&address, remaining(end)?) {
                return Ok(Self { stream, end });
            }
        }
        remaining(end)?;
        Err(error(ErrorKind::Transport))
    }
}
impl Read for Socket {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(remaining(self.end)?))?;
        self.stream.read(bytes)
    }
}
impl Write for Socket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(remaining(self.end)?))?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        remaining(self.end)?;
        self.stream.flush()
    }
}
