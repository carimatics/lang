use super::budget::{Budget, Reservation};
use super::socket::Socket;
use super::*;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::io::Write;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex, TryLockError,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

const WORKER_STACK: usize = 2 * 1024 * 1024;
// The worker's explicit 2 MiB stack plus a conservative 6 MiB allowance for
// bounded rustls handshake/record buffers, HTTP headers, errors, small host/wire
// receipts (each below the 16 KiB response allowance) and handles. These retained
// records are charged here before a transaction's transient workspace retires.
// rustls 0.23.46 caps handshake messages at 0xffff; outgoing buffers are 64 KiB.
const CONNECTION_MEMORY: usize = 8 * 1024 * 1024;
const SERVER_MEMORY: usize = 4 * 1024 * 1024;
const TLS_BUFFER_BYTES: usize = 64 * 1024;

struct Shared {
    service: Service,
    identity: ServiceIdentity,
    tls: Arc<rustls::ServerConfig>,
    budget: Arc<Budget>,
    connect_timeout: Duration,
    request_timeout: Duration,
    // Shared retained data above is dropped before its reservation.
    _reservation: Reservation,
}
enum Service {
    ReadOnly(store::CapturedReads),
    Mutable {
        registry: Mutex<store::MeteredRegistry>,
        clock: Arc<dyn store::Clock + Send + Sync>,
    },
}

struct ServiceClock<'a> {
    inner: &'a dyn store::Clock,
    end: Option<Instant>,
    stopped: Option<&'a AtomicBool>,
}
impl store::Clock for ServiceClock<'_> {
    fn now(&self) -> Result<u64, store::Error> {
        let check = || {
            if self.end.is_some_and(|end| remaining(end).is_err())
                || self
                    .stopped
                    .is_some_and(|flag| flag.load(Ordering::Acquire))
            {
                return Err(store::Error {
                    kind: store::ErrorKind::Io,
                    outcome: store::PublicationOutcome::NotPublished,
                    message: "registry request retired",
                });
            }
            Ok(())
        };
        check()?;
        let time = self.inner.now()?;
        check()?;
        Ok(time)
    }
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

/// A host-only service with explicit read-only and mutable entry points.
/// It owns an explicitly granted listener and registry.
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
        let tls = configuration(certificates, key)?;
        Self::start(
            store::CapturedReads::capture(registry)?,
            listener,
            tls,
            MAX_REQUEST_MEMORY,
            CONNECT_TIMEOUT,
            REQUEST_TIMEOUT,
        )
    }

    /// Explicit mutable service capability with trusted, independently supplied time.
    pub fn spawn_mutable(
        registry: store::Registry,
        listener: TcpListener,
        certificates: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
        clock: Arc<dyn store::Clock + Send + Sync>,
    ) -> Result<Self, Error> {
        let tls = configuration(certificates, key)?;
        let budget = Budget::new(MAX_REQUEST_MEMORY);
        Self::start_mutable(
            registry,
            listener,
            tls,
            budget,
            CONNECT_TIMEOUT,
            REQUEST_TIMEOUT,
            clock,
        )
    }

    fn start_mutable(
        registry: store::Registry,
        listener: TcpListener,
        tls: Arc<rustls::ServerConfig>,
        budget: Arc<Budget>,
        connect_timeout: Duration,
        request_timeout: Duration,
        clock: Arc<dyn store::Clock + Send + Sync>,
    ) -> Result<Self, Error> {
        let reservation = budget.reserve(SERVER_MEMORY)?;
        let time = ServiceClock {
            inner: clock.as_ref(),
            end: None,
            stopped: None,
        };
        let registry = store::MeteredRegistry::with_budget(registry, budget.clone(), &time)?;
        let identity = registry.identity().clone();
        let shared = Arc::new(Shared {
            service: Service::Mutable {
                registry: Mutex::new(registry),
                clock,
            },
            identity,
            tls,
            budget,
            connect_timeout,
            request_timeout,
            _reservation: reservation,
        });
        Self::launch(shared, listener)
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
        let identity = reads.identity().clone();
        let shared = Arc::new(Shared {
            service: Service::ReadOnly(reads),
            identity,
            tls,
            budget,
            connect_timeout,
            request_timeout,
            _reservation: reservation,
        });
        Self::launch(shared, listener)
    }

    fn launch(shared: Arc<Shared>, listener: TcpListener) -> Result<Self, Error> {
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

fn configuration(
    certificates: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<rustls::ServerConfig>, Error> {
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
    Ok(Arc::new(tls))
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
    Metered(store::MeteredBytes),
}
impl Body<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Index(bytes) => bytes,
            Self::Archive { bytes, .. } | Self::Error(bytes) => bytes,
            Self::Metered(bytes) => bytes.as_bytes(),
        }
    }
}
struct Response<'a> {
    status: u16,
    content_type: &'static str,
    body: Body<'a>,
}

fn prepare<'a>(
    request: wire::Request<'_>,
    shared: &Shared,
    reads: &'a store::CapturedReads,
) -> Result<Response<'a>, Error> {
    let (content_type, body) = match request {
        wire::Request::Index(name) => ("application/json", Body::Index(reads.index(name)?)),
        wire::Request::Archive(hash) => {
            let length = reads.archive_length(hash)?;
            let reservation = shared.budget.reserve(length)?;
            let bytes = reads.archive(hash)?;
            (
                "application/x-tar",
                Body::Archive {
                    bytes,
                    _reservation: reservation,
                },
            )
        }
        wire::Request::Publish { .. }
        | wire::Request::Action { .. }
        | wire::Request::Owners { .. } => return Err(error(ErrorKind::NotFound)),
    };
    Ok(Response {
        status: 200,
        content_type,
        body,
    })
}

fn prepare_mutable<'a>(
    request: wire::Request<'_>,
    shared: &Shared,
    registry: &'a mut store::MeteredRegistry,
    stream: &mut impl std::io::Read,
    clock: &ServiceClock<'_>,
    incoming: &mut Option<(Vec<u8>, Reservation)>,
) -> Result<Response<'a>, Error> {
    let mut status = 200;
    let (content_type, body) = match request {
        wire::Request::Index(name) => ("application/json", Body::Index(registry.index(name)?)),
        wire::Request::Archive(hash) => {
            let bytes = registry.archive(hash)?;
            registry.check_head()?;
            ("application/x-tar", Body::Metered(bytes))
        }
        wire::Request::Owners { name, bearer } => {
            let bytes = registry.owners(&bearer, name, clock)?;
            registry.check_head()?;
            ("application/json", Body::Metered(bytes))
        }
        wire::Request::Action {
            name,
            bearer,
            length,
        } => {
            let lease = shared.budget.reserve(length)?;
            let bytes = wire::body(stream, length, 2048, clock.end.expect("request clock"))?;
            *incoming = Some((bytes, lease));
            let (bytes, lease) = incoming.as_ref().expect("admitted body");
            let (receipt, id) = registry.action_http(&bearer, name, bytes, lease, clock)?;
            let bytes = MutationReceipt::from_host(&shared.identity, &id, &receipt)?.encode()?;
            registry.check_head()?;
            ("application/json", Body::Error(bytes))
        }
        wire::Request::Publish {
            name,
            version,
            bearer,
            request_id,
            expected,
            length,
        } => {
            let lease = shared.budget.reserve(length)?;
            let bytes = wire::body(
                stream,
                length,
                super::super::archive::MAX_ARCHIVE_BYTES,
                clock.end.expect("request clock"),
            )?;
            *incoming = Some((bytes, lease));
            let (bytes, lease) = incoming.as_ref().expect("admitted body");
            let (receipt, created) = registry.publish_http(
                store::HttpPublication {
                    bearer: &bearer,
                    bytes,
                    lease,
                    route: (name, version),
                    request_id,
                    expected,
                },
                clock,
            )?;
            status = if created { 201 } else { 200 };
            let bytes =
                MutationReceipt::from_host(&shared.identity, request_id, &receipt)?.encode()?;
            registry.check_head()?;
            ("application/json", Body::Error(bytes))
        }
    };
    Ok(Response {
        status,
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
            stopped: Some(stopped.clone()),
        },
    );
    while stream.conn.is_handshaking() {
        remaining(deadlines.connect)?;
        stream.conn.complete_io(&mut stream.sock)?;
    }
    remaining(deadlines.connect)?;
    stream.sock.end = deadlines.request;
    let header = wire::head(&mut stream, deadlines.request);
    let request = header
        .as_ref()
        .map_err(|e| *e)
        .and_then(|bytes| wire::request(bytes, authority(&shared.identity)));
    match &shared.service {
        Service::ReadOnly(reads) => {
            let prepared = request.and_then(|request| prepare(request, shared, reads));
            let response = match reads.check_head() {
                Ok(()) => prepared.unwrap_or_else(failure),
                Err(e) => {
                    drop(prepared);
                    failure(e.into())
                }
            };
            send_response(&mut stream, response, deadlines.request)
        }
        Service::Mutable { registry, clock } => {
            let mut registry = match registry.try_lock() {
                Ok(registry) => registry,
                Err(TryLockError::WouldBlock) => {
                    return send_response(
                        &mut stream,
                        failure(error(ErrorKind::Busy)),
                        deadlines.request,
                    );
                }
                Err(TryLockError::Poisoned(_)) => {
                    return send_response(
                        &mut stream,
                        failure(error(ErrorKind::State)),
                        deadlines.request,
                    );
                }
            };
            let clock = ServiceClock {
                inner: clock.as_ref(),
                end: Some(deadlines.request),
                stopped: Some(&stopped),
            };
            let mut registry = registry.request_scope(store::RequestControl {
                deadline: deadlines.request,
                stopped: stopped.clone(),
            })?;
            let mut incoming = None;
            let prepared = registry
                .check_head()
                .map_err(Error::from)
                .and(request)
                .and_then(|request| {
                    prepare_mutable(
                        request,
                        shared,
                        &mut registry,
                        &mut stream,
                        &clock,
                        &mut incoming,
                    )
                });
            // The lock, input allocation and its lease all outlive transmission.
            send_response(
                &mut stream,
                prepared.unwrap_or_else(failure),
                deadlines.request,
            )
        }
    }
}

fn send_response(
    stream: &mut rustls::StreamOwned<rustls::ServerConnection, Socket>,
    response: Response<'_>,
    end: Instant,
) -> Result<(), Error> {
    remaining(end)?;
    let reason = match response.status {
        200 => "OK",
        201 => "Created",
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
        remaining(end)?;
        stream.write_all(bytes)?;
    }
    stream.conn.send_close_notify();
    stream.flush()?;
    remaining(end)?;
    Ok(())
}

#[cfg(test)]
mod tests;
