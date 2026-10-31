use super::budget::{Budget, Reservation};
use super::socket::Socket;
use super::*;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::io::Write;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

const WORKER_STACK: usize = 2 * 1024 * 1024;
// The worker's explicit 2 MiB stack plus a conservative 6 MiB allowance for
// bounded rustls handshake/record buffers, HTTP headers, errors and handles.
// rustls 0.23.46 caps handshake messages at 0xffff; outgoing buffers are 64 KiB.
const CONNECTION_MEMORY: usize = 8 * 1024 * 1024;
const SERVER_MEMORY: usize = 4 * 1024 * 1024;
const TLS_BUFFER_BYTES: usize = 64 * 1024;

struct Shared {
    reads: store::CapturedReads,
    tls: Arc<rustls::ServerConfig>,
    budget: Arc<Budget>,
    connect_timeout: Duration,
    request_timeout: Duration,
    // Shared retained data above is dropped before its reservation.
    _reservation: Reservation,
}
struct Control {
    stopped: Arc<AtomicBool>,
    sockets: Mutex<[Option<TcpStream>; MAX_CONNECTIONS]>,
}
impl Control {
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let sockets = self.sockets.lock().unwrap_or_else(|e| e.into_inner());
        for socket in sockets.iter().flatten() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
    fn clear(&self, slot: usize) {
        self.sockets.lock().unwrap_or_else(|e| e.into_inner())[slot] = None;
    }
}
struct Worker {
    slot: usize,
    task: JoinHandle<()>,
    // Held by the acceptor until the worker is actually joined, not released
    // early by the worker when its last response write completes.
    _reservation: Reservation,
}

/// A host-only read service owning an explicitly granted listener and registry.
/// Startup validates a complete immutable generation before accepting peers.
/// Dropping/stopping the server closes active sockets and joins every worker.
pub struct Server {
    address: SocketAddr,
    control: Arc<Control>,
    task: Option<JoinHandle<Result<(), Error>>>,
}
impl Server {
    pub fn spawn(
        registry: store::Registry,
        listener: TcpListener,
        certificates: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
    ) -> Result<Self, Error> {
        if certificates.is_empty()
            || certificates.len() > 8
            || certificates
                .iter()
                .map(|c| c.len())
                .try_fold(0usize, usize::checked_add)
                .is_none_or(|size| size > TLS_BUFFER_BYTES)
            || key.secret_der().len() > TLS_BUFFER_BYTES
        {
            return Err(limit());
        }
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificates, key)
            .map_err(|_| malformed())?;
        tls.alpn_protocols = vec![b"http/1.1".to_vec()];
        tls.session_storage = Arc::new(rustls::server::NoServerSessionStorage {});
        tls.send_tls13_tickets = 0;
        Self::start(
            store::CapturedReads::capture(registry)?,
            listener,
            Arc::new(tls),
            MAX_REQUEST_MEMORY,
            CONNECT_TIMEOUT,
            REQUEST_TIMEOUT,
        )
    }

    fn start(
        reads: store::CapturedReads,
        listener: TcpListener,
        tls: Arc<rustls::ServerConfig>,
        maximum: usize,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self, Error> {
        let budget = Budget::new(maximum);
        let baseline = reads
            .memory_bytes()
            .checked_add(SERVER_MEMORY)
            .ok_or_else(limit)?;
        let reservation = budget.reserve(baseline)?;
        let shared = Arc::new(Shared {
            reads,
            tls,
            budget,
            connect_timeout,
            request_timeout,
            _reservation: reservation,
        });
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let control = Arc::new(Control {
            stopped: Arc::new(AtomicBool::new(false)),
            sockets: Mutex::new(std::array::from_fn(|_| None)),
        });
        let worker_control = control.clone();
        let task = thread::Builder::new()
            .name("registry-accept".into())
            .stack_size(WORKER_STACK)
            .spawn(move || run(listener, shared, worker_control))?;
        Ok(Self {
            address,
            control,
            task: Some(task),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        self.control.stop();
        if let Some(task) = self.task.take() {
            task.thread().unpark();
            task.join().map_err(|_| error(ErrorKind::Transport))??;
        }
        Ok(())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn run(listener: TcpListener, shared: Arc<Shared>, control: Arc<Control>) -> Result<(), Error> {
    let mut workers: Vec<Worker> = Vec::with_capacity(MAX_CONNECTIONS);
    let result = (|| {
        while !control.stopped.load(Ordering::Acquire) {
            let mut i = 0;
            while i < workers.len() {
                if workers[i].task.is_finished() {
                    let worker = workers.swap_remove(i);
                    let result = worker.task.join();
                    control.clear(worker.slot);
                    result.map_err(|_| error(ErrorKind::Transport))?;
                } else {
                    i += 1;
                }
            }
            let stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::park_timeout(Duration::from_millis(5));
                    continue;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            };
            // An accepted socket is either admitted immediately or closed.
            // There is no application queue in front of TLS workers.
            if workers.len() == MAX_CONNECTIONS {
                continue;
            }
            stream.set_nonblocking(false)?;
            let reservation = match shared.budget.reserve(CONNECTION_MEMORY) {
                Ok(reservation) => reservation,
                Err(_) => continue,
            };
            let deadlines = Deadlines::with_limits(shared.connect_timeout, shared.request_timeout);
            let slot = {
                let mut sockets = control.sockets.lock().unwrap_or_else(|e| e.into_inner());
                if control.stopped.load(Ordering::Acquire) {
                    break;
                }
                let slot = sockets
                    .iter()
                    .position(Option::is_none)
                    .expect("bounded joined workers");
                sockets[slot] = Some(stream.try_clone()?);
                slot
            };
            let shared = shared.clone();
            let stopped = control.stopped.clone();
            let task = thread::Builder::new()
                .name("registry-read".into())
                .stack_size(WORKER_STACK)
                .spawn(move || {
                    let _ = serve(stream, &shared, deadlines, stopped);
                });
            match task {
                Ok(task) => workers.push(Worker {
                    slot,
                    task,
                    _reservation: reservation,
                }),
                Err(e) => {
                    control.clear(slot);
                    return Err(e.into());
                }
            }
        }
        Ok(())
    })();
    control.stop();
    let mut panicked = false;
    for worker in workers {
        panicked |= worker.task.join().is_err();
        control.clear(worker.slot);
    }
    if panicked {
        Err(error(ErrorKind::Transport))
    } else {
        result
    }
}

enum Body<'a> {
    Index(&'a [u8]),
    Archive {
        bytes: Vec<u8>,
        _reservation: Reservation,
    },
    Error(Vec<u8>),
}
impl Body<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Index(bytes) => bytes,
            Self::Archive { bytes, .. } | Self::Error(bytes) => bytes,
        }
    }
}
struct Response<'a> {
    status: u16,
    content_type: &'static str,
    body: Body<'a>,
}

fn prepare<'a>(request: wire::Request<'_>, shared: &'a Shared) -> Result<Response<'a>, Error> {
    let (content_type, body) = match request {
        wire::Request::Index(name) => ("application/json", Body::Index(shared.reads.index(name)?)),
        wire::Request::Archive(hash) => {
            let length = shared.reads.archive_length(hash)?;
            let reservation = shared.budget.reserve(length)?;
            let bytes = shared.reads.archive(hash)?;
            (
                "application/x-tar",
                Body::Archive {
                    bytes,
                    _reservation: reservation,
                },
            )
        }
    };
    Ok(Response {
        status: 200,
        content_type,
        body,
    })
}

fn failure<'a>(e: Error) -> Response<'a> {
    #[derive(serde::Serialize)]
    struct Record {
        schema: u32,
        code: &'static str,
        message: &'static str,
        request_id: Option<&'static str>,
    }
    let mut bytes = serde_json::to_vec_pretty(&Record {
        schema: 1,
        code: e.code(),
        message: e.message(),
        request_id: None,
    })
    .expect("fixed error record");
    bytes.push(b'\n');
    let status = match e.kind {
        ErrorKind::Malformed => 400,
        ErrorKind::Limit => 413,
        ErrorKind::Authentication => 401,
        ErrorKind::Authorization => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict | ErrorKind::Integrity | ErrorKind::State => 409,
        ErrorKind::Busy => 429,
        ErrorKind::Transport | ErrorKind::Timeout => 500,
    };
    Response {
        status,
        content_type: "application/json",
        body: Body::Error(bytes),
    }
}

fn serve(
    stream: TcpStream,
    shared: &Shared,
    deadlines: Deadlines,
    stopped: Arc<AtomicBool>,
) -> Result<(), Error> {
    let mut conn = rustls::ServerConnection::new(shared.tls.clone())
        .map_err(|_| error(ErrorKind::Transport))?;
    conn.set_buffer_limit(Some(TLS_BUFFER_BYTES));
    let mut stream = rustls::StreamOwned::new(
        conn,
        Socket {
            stream,
            end: deadlines.connect,
            stopped: Some(stopped),
        },
    );
    while stream.conn.is_handshaking() {
        remaining(deadlines.connect)?;
        stream.conn.complete_io(&mut stream.sock)?;
    }
    remaining(deadlines.connect)?;
    stream.sock.end = deadlines.request;
    let prepared = wire::head(&mut stream, deadlines.request).and_then(|bytes| {
        prepare(
            wire::request(&bytes, authority(shared.reads.identity()))?,
            shared,
        )
    });
    remaining(deadlines.request)?;
    let response = match shared.reads.check_head() {
        Ok(()) => prepared.unwrap_or_else(failure),
        Err(e) => {
            drop(prepared);
            failure(e.into())
        }
    };
    remaining(deadlines.request)?;
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Content Too Large",
        429 => "Too Many Requests",
        _ => "Internal Server Error",
    };
    let body = response.body.bytes();
    let header = format!(
        "HTTP/1.1 {} {reason}\r\nConnection: close\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
        response.status,
        response.content_type,
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    for bytes in body.chunks(TLS_BUFFER_BYTES) {
        remaining(deadlines.request)?;
        stream.write_all(bytes)?;
    }
    stream.conn.send_close_notify();
    stream.flush()?;
    remaining(deadlines.request)?;
    Ok(())
}

#[cfg(test)]
mod tests;
