use super::super::archive::AdmittedArchive;
use super::socket::Socket;
use super::*;
use crate::package::manifest::valid_name;
use rustls::pki_types::ServerName;
use std::io::Write;
use std::sync::Arc;

/// Host-only read capability for exactly one configured origin and explicitly
/// supplied TLS roots. No proxy, redirects, ambient credentials, cache writes,
/// package trust decision or production CLI unsigned-consumption path exists.
pub struct Client {
    identity: ServiceIdentity,
    tls: Arc<rustls::ClientConfig>,
}
impl Client {
    pub fn new(identity: ServiceIdentity, roots: rustls::RootCertStore) -> Self {
        let mut tls = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        tls.alpn_protocols = vec![b"http/1.1".to_vec()];
        Self {
            identity,
            tls: Arc::new(tls),
        }
    }

    pub fn identity(&self) -> &ServiceIdentity {
        &self.identity
    }

    /// Authenticate the network origin and admit its bounded index. Package
    /// selection/signature/freshness authentication remains a separate boundary.
    pub fn index(&self, name: &str) -> Result<store::Index, Error> {
        if !valid_name(name) {
            return Err(malformed());
        }
        let bytes = self.get(
            &format!("/v1/packages/{name}"),
            "application/json",
            store::MAX_INDEX_BYTES,
            None,
            Deadlines::start(),
        )?;
        store::Index::admit(&bytes, &self.identity, name).map_err(Into::into)
    }

    /// Fetch one exact release, binding every indexed field to the admitted
    /// archive. This method cannot silently switch registry/origin or version.
    pub fn archive(&self, index: &store::Index, version: &str) -> Result<AdmittedArchive, Error> {
        if index.identity() != &self.identity {
            return Err(error(ErrorKind::Integrity));
        }
        let release = index
            .release(version)
            .ok_or_else(|| error(ErrorKind::NotFound))?;
        if release.quarantined() {
            return Err(error(ErrorKind::NotFound));
        }
        let length = usize::try_from(release.archive_bytes()).map_err(|_| limit())?;
        let bytes = self.get(
            &format!("/v1/objects/{}", release.archive_sha256()),
            "application/x-tar",
            length,
            Some(length),
            Deadlines::start(),
        )?;
        release.admit_archive(&bytes).map_err(Into::into)
    }

    pub(super) fn get(
        &self,
        path: &str,
        content_type: &str,
        maximum: usize,
        exact: Option<usize>,
        deadlines: Deadlines,
    ) -> Result<Vec<u8>, Error> {
        let authority = authority(&self.identity);
        let (host, port) = authority
            .rsplit_once(':')
            .map_or((authority, 443), |(host, port)| {
                (host, port.parse().expect("validated port"))
            });
        let name = ServerName::try_from(host.to_owned()).map_err(|_| malformed())?;
        let socket = Socket::connect(host, port, deadlines.connect)?;
        let connection = rustls::ClientConnection::new(self.tls.clone(), name)
            .map_err(|_| error(ErrorKind::Transport))?;
        let mut stream = rustls::StreamOwned::new(connection, socket);
        while stream.conn.is_handshaking() {
            remaining(deadlines.connect)?;
            stream.conn.complete_io(&mut stream.sock)?;
        }
        if stream
            .conn
            .alpn_protocol()
            .is_some_and(|protocol| protocol != b"http/1.1")
        {
            return Err(malformed());
        }
        stream.sock.end = deadlines.request;
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
        );
        stream.write_all(request.as_bytes())?;
        stream.flush()?;
        let header = wire::head(&mut stream, deadlines.request)?;
        let response = wire::response(&header, content_type)?;
        if response.status == 200 && exact.is_some_and(|length| response.length != length) {
            return Err(error(ErrorKind::Integrity));
        }
        let maximum = if response.status == 200 {
            maximum
        } else {
            MAX_ERROR_BYTES
        };
        let bytes = wire::body(&mut stream, response.length, maximum, deadlines.request)?;
        wire::eof(&mut stream, deadlines.request)?;
        remaining(deadlines.request)?;
        if response.status != 200 {
            // Never follow Location or interpret arbitrary error text as a
            // destination, credential challenge or user-facing diagnostic.
            return Err(wire::error_body(&bytes, response.status)?);
        }
        Ok(bytes)
    }
}
