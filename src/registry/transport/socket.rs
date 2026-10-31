use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

// A timed-out OS DNS lookup cannot be cancelled portably. Its slot remains held
// until the resolver actually exits, so repeated timeouts cannot spawn an
// unbounded collection of stranded workers. No application lookup queue exists.
static RESOLVERS: AtomicUsize = AtomicUsize::new(0);
const MAX_ADDRESSES: usize = 32;
const CANCELLATION_POLL: Duration = Duration::from_millis(100);

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
    pub(super) stopped: Option<Arc<AtomicBool>>,
}
impl Socket {
    pub(super) fn connect(host: &str, port: u16, end: Instant) -> Result<Self, Error> {
        let addresses = resolve(host, port, end)?;
        for (index, address) in addresses.iter().enumerate() {
            // A failed/unreachable first DNS candidate must not consume the
            // entire shared connect/handshake deadline and starve later ones.
            // In particular, localhost can list IPv6 before an IPv4 listener.
            let candidates = u32::try_from(addresses.len() - index).expect("bounded DNS answers");
            let allowance = (remaining(end)? / candidates).max(Duration::from_nanos(1));
            if let Ok(stream) = TcpStream::connect_timeout(address, allowance) {
                return Ok(Self {
                    stream,
                    end,
                    stopped: None,
                });
            }
        }
        remaining(end)?;
        Err(error(ErrorKind::Transport))
    }

    fn allowance(&self) -> io::Result<Duration> {
        if self
            .stopped
            .as_ref()
            .is_some_and(|stop| stop.load(Ordering::Acquire))
        {
            return Err(io::Error::from(io::ErrorKind::ConnectionAborted));
        }
        let remaining = remaining(self.end)?;
        Ok(if self.stopped.is_some() {
            remaining.min(CANCELLATION_POLL)
        } else {
            remaining
        })
    }

    fn retry(&self, error: &io::Error) -> bool {
        self.stopped.is_some()
            && matches!(
                error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            )
    }
}
impl Read for Socket {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        // Shutdown of a cloned socket need not wake an outstanding read on
        // every host. Poll cancellation independently, without renewing the
        // absolute TLS/request deadline or retiring the worker's reservation.
        loop {
            self.stream.set_read_timeout(Some(self.allowance()?))?;
            match self.stream.read(bytes) {
                Err(error) if self.retry(&error) => {}
                result => return result,
            }
        }
    }
}
impl Write for Socket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.stream.set_write_timeout(Some(self.allowance()?))?;
            match self.stream.write(bytes) {
                Err(error) if self.retry(&error) => {}
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.allowance()?;
        self.stream.flush()
    }
}
