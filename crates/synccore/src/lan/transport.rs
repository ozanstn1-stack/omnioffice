//! Pinned-TLS transport for the LAN sync (C14).
//!
//! Every device answers on a TCP port with a self-signed certificate it
//! generated itself. There is no CA: a peer is trusted only when the
//! certificate's SHA-256 fingerprint matches the one recorded at pairing, and
//! a pairing connection is the one moment where an unknown certificate is
//! accepted (the response is then checked against the fingerprint the host
//! reports and the shared key it returns, in `super::lan`).
//!
//! Messages are length-prefixed frames: a 4-byte big-endian JSON header and an
//! optional raw body of the length the header announces. Bodies are capped at
//! [`crate::MAX_TRANSFER_BYTES`], headers at [`MAX_FRAME_BYTES`]; a peer that
//! announces more is dropped instead of being buffered.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::SyncError;

/// Largest JSON header we accept, so a hostile peer cannot make us buffer.
pub(super) const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// The name the self-signed certificate carries; pinning makes it cosmetic.
pub(super) const TLS_NAME: &str = "omnioffice-lan";

/// Socket timeout for one request/response exchange.
const IO_TIMEOUT: Duration = Duration::from_secs(60);

/// A header type announces how many raw body bytes follow it.
pub(super) trait HasBodyLen {
    fn body_len(&self) -> u64;
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// SHA-256 of a certificate's DER encoding, lowercase hex. This is the pin.
pub(super) fn fingerprint(cert: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    crate::metadata::hex_lower(&Sha256::digest(cert))
}

/// A client-side verifier that accepts exactly one certificate fingerprint.
#[derive(Debug)]
struct PinnedServerCert {
    pin: String,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedServerCert {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if fingerprint(end_entity.as_ref()) == self.pin {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// A pairing-only client verifier: it accepts whatever certificate the host
/// presents and records its fingerprint, which the caller then compares with
/// the fingerprint the host claims in its pairing response.
#[derive(Debug, Clone)]
pub(super) struct RecordingServerCert {
    seen: Arc<Mutex<Option<String>>>,
    provider: Arc<CryptoProvider>,
}

impl RecordingServerCert {
    fn new() -> Self {
        Self { seen: Arc::new(Mutex::new(None)), provider: provider() }
    }

    /// The fingerprint of the certificate the server presented, once the
    /// handshake has run.
    pub(super) fn seen(&self) -> Option<String> {
        self.seen.lock().ok().and_then(|value| value.clone())
    }
}

impl ServerCertVerifier for RecordingServerCert {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(fingerprint(end_entity.as_ref()));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

pub(super) fn server_config(
    certificate_der: &[u8],
    private_key_der: &[u8],
) -> Result<Arc<rustls::ServerConfig>, SyncError> {
    let certs = vec![CertificateDer::from(certificate_der.to_vec())];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(private_key_der.to_vec()));
    let config = rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|error| SyncError::Network(format!("TLS setup failed: {error}")))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| SyncError::Network(format!("TLS certificate rejected: {error}")))?;
    Ok(Arc::new(config))
}

/// One client connection to a peer.
pub(super) struct Client {
    stream: rustls::StreamOwned<rustls::ClientConnection, TcpStream>,
    /// Fingerprint of the certificate the server presented.
    pub(super) server_fingerprint: String,
}

impl Client {
    /// Opens a connection. `pin` is `Some` for every ordinary request and
    /// `None` during pairing (the fingerprint is then recorded and checked
    /// against the host's own claim).
    pub(super) fn connect(address: &str, pin: Option<&str>) -> Result<Self, SyncError> {
        let socket = TcpStream::connect(address)
            .map_err(|error| SyncError::Network(format!("cannot reach {address}: {error}")))?;
        socket.set_read_timeout(Some(IO_TIMEOUT)).ok();
        socket.set_write_timeout(Some(IO_TIMEOUT)).ok();
        socket.set_nodelay(true).ok();
        let config = rustls::ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|error| SyncError::Network(format!("TLS setup failed: {error}")))?;
        let recording = RecordingServerCert::new();
        let verifier: Arc<dyn ServerCertVerifier> = match pin {
            Some(pin) => Arc::new(PinnedServerCert { pin: pin.to_string(), provider: provider() }),
            None => Arc::new(recording.clone()),
        };
        let config = config.dangerous().with_custom_certificate_verifier(verifier).with_no_client_auth();
        let name = ServerName::try_from(TLS_NAME).map_err(|_| SyncError::Protocol("invalid TLS name".into()))?;
        let connection = rustls::ClientConnection::new(Arc::new(config), name)
            .map_err(|error| SyncError::Network(format!("TLS handshake failed: {error}")))?;
        let mut stream = rustls::StreamOwned::new(connection, socket);
        // Drive the handshake now, so a pin mismatch or a dead peer fails
        // before the caller builds a request.
        while stream.conn.is_handshaking() {
            stream.conn.complete_io(&mut stream.sock).map_err(net)?;
        }
        let server_fingerprint = match pin {
            Some(pin) => pin.to_string(),
            None => recording.seen().ok_or_else(|| SyncError::Protocol("the peer presented no certificate".into()))?,
        };
        Ok(Self { stream, server_fingerprint })
    }

    /// Sends one request and reads its response.
    pub(super) fn request<T, R>(&mut self, header: &T, body: Option<&[u8]>) -> Result<(R, Vec<u8>), SyncError>
    where
        T: Serialize + HasBodyLen,
        R: DeserializeOwned + HasBodyLen,
    {
        write_frame(&mut self.stream, header, body)?;
        let response = read_frame(&mut self.stream);
        self.stream.conn.send_close_notify();
        let _ = self.stream.conn.complete_io(&mut self.stream.sock);
        response
    }
}

/// One server-side connection: exactly one request, then the caller closes it.
pub(super) struct ServerPeer {
    stream: rustls::StreamOwned<rustls::ServerConnection, TcpStream>,
}

impl ServerPeer {
    /// Completes the TLS handshake on an accepted socket.
    pub(super) fn accept_stream(socket: TcpStream, config: &Arc<rustls::ServerConfig>) -> Result<Self, SyncError> {
        // On macOS an accepted socket inherits the listener's non-blocking
        // mode; the handshake below is synchronous, so force blocking I/O.
        socket.set_nonblocking(false).ok();
        socket.set_read_timeout(Some(IO_TIMEOUT)).ok();
        socket.set_write_timeout(Some(IO_TIMEOUT)).ok();
        let connection = rustls::ServerConnection::new(config.clone())
            .map_err(|error| SyncError::Network(format!("TLS setup failed: {error}")))?;
        let mut stream = rustls::StreamOwned::new(connection, socket);
        while stream.conn.is_handshaking() {
            stream.conn.complete_io(&mut stream.sock).map_err(net)?;
        }
        Ok(Self { stream })
    }

    /// Reads one request frame.
    pub(super) fn read_request<T: DeserializeOwned + HasBodyLen>(&mut self) -> Result<(T, Vec<u8>), SyncError> {
        read_frame(&mut self.stream)
    }

    /// Writes one response frame and closes the TLS session politely.
    pub(super) fn respond<R: Serialize + HasBodyLen>(
        &mut self,
        header: &R,
        body: Option<&[u8]>,
    ) -> Result<(), SyncError> {
        write_frame(&mut self.stream, header, body)?;
        self.stream.conn.send_close_notify();
        let _ = self.stream.conn.complete_io(&mut self.stream.sock);
        Ok(())
    }
}

fn write_frame<S, T>(stream: &mut S, header: &T, body: Option<&[u8]>) -> Result<(), SyncError>
where
    S: Read + Write,
    T: Serialize + HasBodyLen,
{
    let encoded = serde_json::to_vec(header).map_err(|error| SyncError::Protocol(error.to_string()))?;
    if encoded.len() > MAX_FRAME_BYTES {
        return Err(SyncError::Protocol("the frame header is too large".into()));
    }
    let body_len = header.body_len();
    if body_len > crate::MAX_TRANSFER_BYTES {
        return Err(SyncError::TooLarge(body_len));
    }
    let length = u32::try_from(encoded.len()).map_err(|_| SyncError::Protocol("frame length overflow".into()))?;
    stream.write_all(&length.to_be_bytes()).map_err(net)?;
    stream.write_all(&encoded).map_err(net)?;
    if let Some(body) = body {
        stream.write_all(body).map_err(net)?;
    }
    stream.flush().map_err(net)
}

fn read_frame<R, T>(stream: &mut T) -> Result<(R, Vec<u8>), SyncError>
where
    T: Read + Write,
    R: DeserializeOwned + HasBodyLen,
{
    let payload = read_header(stream)?;
    let header: R = serde_json::from_slice(&payload).map_err(|error| SyncError::Protocol(error.to_string()))?;
    let body_len = header.body_len();
    if body_len > crate::MAX_TRANSFER_BYTES {
        return Err(SyncError::TooLarge(body_len));
    }
    let mut body = Vec::new();
    if body_len > 0 {
        body.resize(body_len as usize, 0);
        stream.read_exact(&mut body).map_err(net)?;
    }
    Ok((header, body))
}

fn read_header<T: Read>(stream: &mut T) -> Result<Vec<u8>, SyncError> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length).map_err(net)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(SyncError::Protocol("the peer sent an impossible frame size".into()));
    }
    let mut buffer = vec![0u8; length];
    stream.read_exact(&mut buffer).map_err(net)?;
    Ok(buffer)
}

fn net(error: std::io::Error) -> SyncError {
    SyncError::Network(error.to_string())
}
