//! Shared gRPC TLS primitives for our rustls based transports.
//!
//! The control-plane and io-engine speak gRPC over TLS between each other and amongst their own
//! components, with two transport modes:
//!
//! - **auto-TLS**: an ephemeral, in-memory self-signed server certificate, with the client
//!   bypassing certificate verification (but still verifying the handshake signature). This gives
//!   encryption without a PKI, for bringing up encrypted transport before certificates are
//!   provisioned.
//! - **file-backed (mutual) TLS**: certificate/key (and optional CA) files on disk, see
//!   [`TlsConfig`]. The server verifies the client certificate against the CA when one is
//!   configured (mutual TLS), and the client verifies the server against the CA (or the system
//!   trust roots when none is set) and presents its own certificate for mutual TLS. File-backed
//!   material is reloaded when the files rotate on disk, see [`Reloadable`].
//!
//! On the server side, a [`Listener`] accepts connections for tonic with the handshakes performed
//! concurrently. It serves TLS exclusively ([`Listener::tls_incoming`]) unless a server sitting at
//! a mixed-version boundary explicitly opts into sniffing plaintext clients on the same port
//! ([`Listener::sniffing_incoming`]) so a plaintext to TLS rolling upgrade keeps working.
//!
//! The FIPS crypto provider and its signature algorithms come from the sibling `fips` crate so
//! every stack verifies handshakes the same way.

pub mod pki;

/// Re-exported so consumers can name the configuration types without depending on the exact
/// rustls version this crate is built against.
pub use rustls;

use futures::StreamExt;
use hyper_util::rt::TokioIo;
use std::{
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, PoisonError, RwLock},
    task::{Context as TaskContext, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tokio_rustls::{server::TlsStream, TlsAcceptor, TlsConnector};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Endpoint};

/// Which certificate file a TLS operation was reading.
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum CertFile {
    /// An identity certificate chain.
    #[error("certificate")]
    Certificate,
    /// A CA bundle used as the trust roots to verify the server.
    #[error("CA file")]
    CaFile,
    /// A client CA bundle used to verify client certificates.
    #[error("client CA file")]
    ClientCaFile,
}

/// Which certificate/key pair failed to validate.
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum Identity {
    /// The ephemeral self-signed auto-TLS certificate.
    #[error("self-signed certificate/key")]
    SelfSigned,
    /// The client certificate presented for mutual TLS.
    #[error("client certificate/key")]
    Client,
    /// The server certificate.
    #[error("certificate/key")]
    Server,
}

/// A certificate generation or signing operation (via rcgen).
#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum CertOp {
    /// Generating the ephemeral self-signed auto-TLS certificate.
    #[error("failed to generate self-signed certificate")]
    GenerateSelfSigned,
    /// Generating the CA key pair.
    #[error("failed to generate the CA key pair")]
    GenerateCaKeyPair,
    /// Self-signing the CA certificate.
    #[error("failed to self-sign the CA certificate")]
    SelfSignCa,
    /// Parsing the subject alternative names of a leaf certificate.
    #[error("invalid subject alternative names")]
    SubjectAltNames,
    /// Generating a leaf certificate key pair.
    #[error("failed to generate the key pair")]
    GenerateKeyPair,
    /// Signing a leaf certificate with the CA.
    #[error("failed to sign the certificate")]
    SignCertificate,
}

/// An error building TLS material, e.g. from unreadable or invalid files.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to bind the gRPC socket.
    #[error("failed to bind gRPC socket to {socket}: {source}")]
    Bind {
        /// The socket that could not be bound.
        socket: SocketAddr,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// Failed to open a TLS certificate or key file.
    #[error("failed to open {}: {source}", path.display())]
    OpenFile {
        /// The file that could not be opened.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// Failed to read certificates from a TLS file.
    #[error("failed to read TLS {file}: {source}")]
    ReadCertificates {
        /// Which certificate file was being read.
        file: CertFile,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// Failed to read the TLS private key.
    #[error("failed to read the TLS private key: {source}")]
    ReadPrivateKey {
        /// The underlying I/O error.
        source: io::Error,
    },
    /// The TLS key file contained no private key.
    #[error("no private key found in the TLS key file")]
    MissingPrivateKey,
    /// No valid certificates were found in a TLS file.
    #[error("no valid certificates found in the TLS {file}")]
    NoValidCertificates {
        /// Which certificate file was being read.
        file: CertFile,
    },
    /// No system trust roots could be loaded for TLS.
    #[error("no system trust roots could be loaded for TLS")]
    NoSystemTrustRoots,
    /// Failed to build the TLS client certificate verifier.
    #[error("failed to build the TLS client certificate verifier: {source}")]
    ClientVerifier {
        /// The underlying rustls error.
        source: rustls::server::VerifierBuilderError,
    },
    /// Invalid TLS certificate/key material.
    #[error("invalid TLS {identity}: {source}")]
    InvalidCertificate {
        /// Which identity was invalid.
        identity: Identity,
        /// The underlying rustls error.
        source: rustls::Error,
    },
    /// Both the TLS certificate and private key files must be specified together.
    #[error("both the TLS certificate and private key files must be specified")]
    IncompleteIdentity,
    /// A TLS server requires both a certificate and a private key.
    #[error("a TLS server requires both certificate and private key files")]
    MissingServerIdentity,
    /// Failed to fingerprint the TLS certificate files.
    #[error("failed to fingerprint the TLS certificate files: {source}")]
    Fingerprint {
        /// The underlying I/O error.
        source: io::Error,
    },
    /// Failed to generate TLS certificate material.
    #[error("{op}: {source}")]
    Certificate {
        /// The operation that was being performed.
        op: CertOp,
        /// The underlying certificate generation error.
        source: rcgen::Error,
    },
    /// Failed to establish the TLS channel to the endpoint.
    #[error("failed to connect the TLS channel: {source}")]
    Connect {
        /// The underlying tonic transport error.
        source: tonic::transport::Error,
    },
}

/// A server certificate verifier that does not authenticate the server identity, but still
/// verifies the handshake signature against the presented certificate's key.
///
/// This is used in auto-TLS mode, where the server serves an ephemeral self-signed certificate
/// that no CA vouches for. The handshake signature is still checked, using the installed crypto
/// provider's algorithms, so only FIPS-approved ones are used in FIPS mode.
#[derive(Debug)]
struct NoCertificateVerification;

impl rustls::client::danger::ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer,
        _intermediates: &[rustls::pki_types::CertificateDer],
        _server_name: &rustls::pki_types::ServerName,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer,
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            signature,
            &fips::signature_verification_algorithms(),
        )
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer,
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            signature,
            &fips::signature_verification_algorithms(),
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        fips::supported_signature_schemes()
    }
}

/// The ALPN protocol of gRPC's HTTP/2 transport.
const H2_ALPN: &[u8] = b"h2";

/// Build a client TLS configuration that bypasses server certificate verification (auto-TLS).
pub fn auto_client_config() -> rustls::ClientConfig {
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
    config
        .dangerous()
        .set_certificate_verifier(Arc::new(NoCertificateVerification));
    config.alpn_protocols = vec![H2_ALPN.to_vec()];
    config
}

/// Generate an ephemeral self-signed server configuration for auto-TLS.
///
/// Because its certificate only exists in memory, this configuration cannot be reloaded and does
/// not support client authentication.
pub fn auto_server_config(subject_alt_names: Vec<String>) -> Result<rustls::ServerConfig, Error> {
    use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};

    let material = rcgen::generate_simple_self_signed(subject_alt_names).map_err(|source| {
        Error::Certificate {
            op: CertOp::GenerateSelfSigned,
            source,
        }
    })?;
    let certificate = material.cert.der().clone();
    let private_key =
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(material.key_pair.serialize_der()));

    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], private_key)
        .map_err(|source| Error::InvalidCertificate {
            identity: Identity::SelfSigned,
            source,
        })?;
    config.alpn_protocols = vec![H2_ALPN.to_vec()];
    Ok(config)
}

fn open(path: &Path) -> Result<io::BufReader<std::fs::File>, Error> {
    std::fs::File::open(path)
        .map(io::BufReader::new)
        .map_err(|source| Error::OpenFile {
            path: path.to_path_buf(),
            source,
        })
}

fn read_certificates(
    path: &Path,
    file: CertFile,
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, Error> {
    rustls_pemfile::certs(&mut open(path)?)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| Error::ReadCertificates { file, source })
}

fn read_private_key(path: &Path) -> Result<rustls::pki_types::PrivateKeyDer<'static>, Error> {
    rustls_pemfile::private_key(&mut open(path)?)
        .map_err(|source| Error::ReadPrivateKey { source })?
        .ok_or(Error::MissingPrivateKey)
}

/// The trust roots used to verify a peer: the given CA bundle, or the system trust roots when none
/// is configured.
fn root_store(
    ca_certificate: Option<&Path>,
    file: CertFile,
) -> Result<rustls::RootCertStore, Error> {
    let mut roots = rustls::RootCertStore::empty();
    let certificates = match ca_certificate {
        Some(ca_certificate) => read_certificates(ca_certificate, file)?,
        None => rustls_native_certs::load_native_certs().certs,
    };
    let (valid, _) = roots.add_parsable_certificates(certificates);
    if valid == 0 {
        return Err(match ca_certificate {
            Some(_) => Error::NoValidCertificates { file },
            None => Error::NoSystemTrustRoots,
        });
    }
    Ok(roots)
}

/// Build a client TLS configuration from certificate files.
///
/// The CA bundle, if provided, replaces the default trust roots used to verify the server
/// certificate; otherwise the system trust roots are used. A certificate/key pair, if provided, is
/// presented to the server for mutual TLS.
pub fn file_client_config(
    ca_certificate: Option<&Path>,
    certificate: Option<&Path>,
    private_key: Option<&Path>,
) -> Result<rustls::ClientConfig, Error> {
    let roots = root_store(ca_certificate, CertFile::CaFile)?;
    let builder = rustls::ClientConfig::builder().with_root_certificates(roots);
    let mut config = match (certificate, private_key) {
        (Some(certificate), Some(private_key)) => builder
            .with_client_auth_cert(
                read_certificates(certificate, CertFile::Certificate)?,
                read_private_key(private_key)?,
            )
            .map_err(|source| Error::InvalidCertificate {
                identity: Identity::Client,
                source,
            })?,
        _ => builder.with_no_client_auth(),
    };
    config.alpn_protocols = vec![H2_ALPN.to_vec()];
    Ok(config)
}

/// Build a server TLS configuration from certificate files.
///
/// When a client CA bundle is provided the server requires and verifies client certificates
/// against it (mutual TLS); otherwise client authentication is not requested.
pub fn file_server_config(
    certificate: &Path,
    private_key: &Path,
    client_ca_certificate: Option<&Path>,
) -> Result<rustls::ServerConfig, Error> {
    let certificates = read_certificates(certificate, CertFile::Certificate)?;
    let private_key = read_private_key(private_key)?;

    let builder = rustls::ServerConfig::builder();
    let mut config = match client_ca_certificate {
        Some(ca_certificate) => {
            let roots = root_store(Some(ca_certificate), CertFile::ClientCaFile)?;
            let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
                .build()
                .map_err(|source| Error::ClientVerifier { source })?;
            builder
                .with_client_cert_verifier(verifier)
                .with_single_cert(certificates, private_key)
        }
        None => builder
            .with_no_client_auth()
            .with_single_cert(certificates, private_key),
    }
    .map_err(|source| Error::InvalidCertificate {
        identity: Identity::Server,
        source,
    })?;
    config.alpn_protocols = vec![H2_ALPN.to_vec()];
    Ok(config)
}

/// TLS certificate files used by a gRPC endpoint.
///
/// The files are intentionally retained as paths rather than loaded into memory so a configuration
/// can be rebuilt after the certificates are rotated on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TlsConfig {
    ca_certificate: Option<PathBuf>,
    certificate: Option<PathBuf>,
    private_key: Option<PathBuf>,
}

impl TlsConfig {
    /// Create a TLS configuration from optional CA and client/server identity files. The
    /// certificate and private key must be provided together.
    pub fn new(
        ca_certificate: Option<PathBuf>,
        certificate: Option<PathBuf>,
        private_key: Option<PathBuf>,
    ) -> Result<Self, Error> {
        if certificate.is_some() != private_key.is_some() {
            return Err(Error::IncompleteIdentity);
        }
        Ok(Self {
            ca_certificate,
            certificate,
            private_key,
        })
    }

    /// Whether TLS has been enabled for this endpoint.
    pub fn enabled(&self) -> bool {
        self.ca_certificate.is_some() || self.certificate.is_some()
    }

    /// The CA bundle, used to verify the peer.
    pub fn ca_certificate(&self) -> Option<&Path> {
        self.ca_certificate.as_deref()
    }
    /// The certificate chain presented to the peer.
    pub fn certificate(&self) -> Option<&Path> {
        self.certificate.as_deref()
    }
    /// The private key of the certificate presented to the peer.
    pub fn private_key(&self) -> Option<&Path> {
        self.private_key.as_deref()
    }

    /// Certificate files that must be watched for automatic reload.
    pub fn paths(&self) -> Vec<PathBuf> {
        [
            self.ca_certificate.clone(),
            self.certificate.clone(),
            self.private_key.clone(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Filesystem targets to watch for certificate rotation.
    ///
    /// Kubernetes projected secrets replace directory entries atomically, so watch their shared
    /// parent directory when all TLS files live below it.
    pub fn watch_targets(&self) -> Vec<PathBuf> {
        cert_watcher::watch_targets(self.paths())
    }

    /// Last-modified fingerprint of the files, used to skip redundant reloads.
    pub fn fingerprint(&self) -> io::Result<Vec<std::time::SystemTime>> {
        cert_watcher::fingerprint(&self.paths())
    }

    /// Build a fresh client TLS configuration from the current file contents, see
    /// [`file_client_config`].
    pub fn client_config(&self) -> Result<rustls::ClientConfig, Error> {
        file_client_config(
            self.ca_certificate(),
            self.certificate(),
            self.private_key(),
        )
    }

    /// Build a fresh server TLS configuration from the current file contents, see
    /// [`file_server_config`].
    pub fn server_config(&self) -> Result<rustls::ServerConfig, Error> {
        let (Some(certificate), Some(private_key)) = (&self.certificate, &self.private_key) else {
            return Err(Error::MissingServerIdentity);
        };
        file_server_config(certificate, private_key, self.ca_certificate())
    }
}

/// A value built from file-backed TLS material which is rebuilt when the files change on disk.
///
/// This backs servers ([`rustls::ServerConfig`]) and clients ([`rustls::ClientConfig`]); the
/// `build` function supplies the construction from the current certificate files. The latest value
/// is read via [`Reloadable::get`] each time a connection is established, so new/reconnecting
/// connections use rotated certificates.
pub struct Reloadable<T> {
    current: Arc<RwLock<Arc<T>>>,
}

impl<T> Clone for Reloadable<T> {
    fn clone(&self) -> Self {
        Self {
            current: self.current.clone(),
        }
    }
}

type Build<T> = Box<dyn Fn(&TlsConfig) -> Result<T, Error> + Send + Sync>;

struct ReloadState<T> {
    tls: TlsConfig,
    fingerprint: Vec<std::time::SystemTime>,
    current: Arc<RwLock<Arc<T>>>,
    build: Build<T>,
}

impl<T: Send + Sync + 'static> Reloadable<T> {
    /// Build the initial value and spawn a watcher that rebuilds it whenever the certificate files
    /// rotate on disk.
    pub fn new(
        tls: TlsConfig,
        watcher_name: &str,
        build: impl Fn(&TlsConfig) -> Result<T, Error> + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let current = Arc::new(RwLock::new(Arc::new(build(&tls)?)));
        let targets = tls.watch_targets();
        let paths = tls.paths();
        let state = Arc::new(RwLock::new(ReloadState {
            fingerprint: tls.fingerprint().unwrap_or_default(),
            tls,
            current: current.clone(),
            build: Box::new(build),
        }));
        cert_watcher::spawn(watcher_name, targets, move || {
            Self::reload_logged(&state, &paths)
        });
        Ok(Self { current })
    }

    /// The current value, cloned cheaply (it is behind an [`Arc`]).
    pub fn get(&self) -> Arc<T> {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn reload(state: &Arc<RwLock<ReloadState<T>>>) -> Result<bool, Error> {
        let state_guard = state.read().unwrap_or_else(PoisonError::into_inner);
        let fingerprint = state_guard
            .tls
            .fingerprint()
            .map_err(|source| Error::Fingerprint { source })?;
        if fingerprint == state_guard.fingerprint {
            return Ok(false);
        }
        let value = Arc::new((state_guard.build)(&state_guard.tls)?);
        *state_guard
            .current
            .write()
            .unwrap_or_else(PoisonError::into_inner) = value;
        drop(state_guard);
        state
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .fingerprint = fingerprint;
        Ok(true)
    }

    fn reload_logged(state: &Arc<RwLock<ReloadState<T>>>, paths: &[PathBuf]) {
        match Self::reload(state) {
            Ok(true) => tracing::info!(?paths, "Reloaded gRPC TLS certificates"),
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(?paths, %error, "Failed to reload gRPC TLS certificates")
            }
        }
    }
}

/// Establish a single TLS connection to the endpoint described by `uri` using the given connector,
/// wrapping the stream for use with hyper/tonic.
async fn tls_connect_io(
    tls: TlsConnector,
    uri: http::Uri,
) -> io::Result<TokioIo<tokio_rustls::client::TlsStream<TcpStream>>> {
    let host = uri
        .host()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "gRPC endpoint has no host"))?;
    let port = uri.port_u16().unwrap_or(443);
    let stream = TcpStream::connect((host, port)).await?;
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    tls.connect(server_name, stream)
        .await
        .map(TokioIo::new)
        .map_err(io::Error::other)
}

/// Eagerly establish a TLS channel to `endpoint` using the given client configuration, for example
/// one built with [`file_client_config`].
///
/// The endpoint must use an `http` scheme so tonic does not apply (and reject) its own TLS logic;
/// the TLS handshake is performed by the custom connector. This applies to every connector in this
/// crate.
pub async fn tls_connect(
    endpoint: &Endpoint,
    config: Arc<rustls::ClientConfig>,
) -> Result<Channel, tonic::transport::Error> {
    let tls = TlsConnector::from(config);
    let connector = tower::service_fn(move |uri: http::Uri| tls_connect_io(tls.clone(), uri));
    endpoint.connect_with_connector(connector).await
}

/// Lazily establish a TLS channel to `endpoint` using the given client configuration. The
/// connection is deferred until the first request.
pub fn tls_connect_lazy(endpoint: &Endpoint, config: Arc<rustls::ClientConfig>) -> Channel {
    let tls = TlsConnector::from(config);
    let connector = tower::service_fn(move |uri: http::Uri| tls_connect_io(tls.clone(), uri));
    endpoint.connect_with_connector_lazy(connector)
}

/// Eagerly establish an auto-TLS channel to `endpoint`, bypassing certificate verification.
pub async fn auto_tls_connect(endpoint: &Endpoint) -> Result<Channel, tonic::transport::Error> {
    tls_connect(endpoint, Arc::new(auto_client_config())).await
}

/// Lazily establish an auto-TLS channel to `endpoint`, bypassing certificate verification. The
/// connection is deferred until the first request.
pub fn auto_tls_connect_lazy(endpoint: &Endpoint) -> Channel {
    tls_connect_lazy(endpoint, Arc::new(auto_client_config()))
}

/// Eagerly establish a file-backed TLS channel to `endpoint`.
///
/// The server certificate is verified against the configured CA (or the system trust roots), and a
/// client certificate is presented for mutual TLS when the configuration includes one. The files
/// are read for every connection, so a reconnect after certificate rotation uses the latest
/// material.
pub async fn file_tls_connect(endpoint: &Endpoint, tls: &TlsConfig) -> Result<Channel, Error> {
    let config = Arc::new(tls.client_config()?);
    tls_connect(endpoint, config)
        .await
        .map_err(|source| Error::Connect { source })
}

/// Lazily establish a file-backed TLS channel to `endpoint`, see [`file_tls_connect`]. The TLS
/// material is reloaded when the files rotate on disk, so reconnections pick up the latest
/// certificates.
pub fn file_tls_connect_lazy(endpoint: &Endpoint, tls: &TlsConfig) -> Result<Channel, Error> {
    let config = Reloadable::new(tls.clone(), "grpc-tls-client-cert-watcher", |tls| {
        tls.client_config()
    })?;
    let connector = tower::service_fn(move |uri: http::Uri| {
        tls_connect_io(TlsConnector::from(config.get()), uri)
    });
    Ok(endpoint.connect_with_connector_lazy(connector))
}

/// The content type of a TLS handshake record, which is the first byte of every TLS `ClientHello`.
const TLS_HANDSHAKE_RECORD: u8 = 0x16;

/// A TLS gRPC server connection, as accepted by [`Listener::tls_incoming`].
///
/// This is the rustls stream itself, for which tonic exposes the peer certificates through its
/// connect info.
pub type TlsConnection = TlsStream<TcpStream>;

/// A gRPC server connection accepted by a sniffing listener ([`Listener::sniffing_incoming`]),
/// which serves TLS and plaintext on the same port.
///
/// The transport is chosen by peeking the first byte of the stream: a TLS `ClientHello` record
/// always starts with `0x16`, whereas cleartext HTTP/2 (h2c) starts with the connection preface
/// `PRI * HTTP/2.0` (i.e. `0x50`). This lets a single listener accept legacy plaintext clients and
/// TLS clients at the same time, which is what allows a plaintext to TLS rolling upgrade.
///
/// Only a server at a mixed-version boundary should accept this: everything else must use
/// [`TlsConnection`] so a configured TLS transport is never silently downgraded.
pub enum MaybeTlsConnection {
    /// A plaintext connection.
    Plain(Pin<Box<TcpStream>>),
    /// A TLS connection.
    Tls(Pin<Box<TlsConnection>>),
}

impl AsyncRead for MaybeTlsConnection {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTlsConnection::Plain(stream) => stream.as_mut().poll_read(cx, buf),
            MaybeTlsConnection::Tls(stream) => stream.as_mut().poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeTlsConnection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeTlsConnection::Plain(stream) => stream.as_mut().poll_write(cx, buf),
            MaybeTlsConnection::Tls(stream) => stream.as_mut().poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTlsConnection::Plain(stream) => stream.as_mut().poll_flush(cx),
            MaybeTlsConnection::Tls(stream) => stream.as_mut().poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTlsConnection::Plain(stream) => stream.as_mut().poll_shutdown(cx),
            MaybeTlsConnection::Tls(stream) => stream.as_mut().poll_shutdown(cx),
        }
    }
}

impl tonic::transport::server::Connected for MaybeTlsConnection {
    type ConnectInfo = ();

    fn connect_info(&self) -> Self::ConnectInfo {}
}

/// Complete a TLS handshake on an accepted connection.
///
/// Any failure is mapped to a recoverable error kind so a single bad connection never tears down
/// the listener.
async fn accept_tls(connection: TcpStream, acceptor: TlsAcceptor) -> io::Result<TlsConnection> {
    acceptor
        .accept(connection)
        .await
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Complete a TLS handshake on an accepted connection, or when the first byte is not a TLS record,
/// use the connection as plaintext.
async fn accept_sniffing(
    connection: TcpStream,
    acceptor: TlsAcceptor,
) -> io::Result<MaybeTlsConnection> {
    let mut first = [0u8; 1];
    let peeked = connection
        .peek(&mut first)
        .await
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if peeked == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "connection closed before any data was received",
        ));
    }
    if first[0] != TLS_HANDSHAKE_RECORD {
        return Ok(MaybeTlsConnection::Plain(Box::pin(connection)));
    }
    accept_tls(connection, acceptor)
        .await
        .map(|stream| MaybeTlsConnection::Tls(Box::pin(stream)))
}

/// Maximum number of connections whose TLS sniff/handshake may be in progress at once.
///
/// This bounds the work performed off the accept path and provides backpressure: once this many
/// handshakes are in flight, no further connections are accepted until one completes.
const MAX_CONCURRENT_HANDSHAKES: usize = 256;

/// The stream of accepted gRPC server connections of a TLS-only listener.
pub type TlsIncoming = Pin<Box<dyn futures::Stream<Item = io::Result<TlsConnection>> + Send>>;

/// The stream of accepted gRPC server connections of a sniffing listener.
pub type MaybeTlsIncoming =
    Pin<Box<dyn futures::Stream<Item = io::Result<MaybeTlsConnection>> + Send>>;

type Acceptor = Box<dyn Fn() -> TlsAcceptor + Send + 'static>;

/// A bound gRPC TLS listener, which yields its accepted connections to tonic through
/// [`Listener::tls_incoming`] or [`Listener::sniffing_incoming`].
///
/// The TLS handshakes are performed concurrently, with up to [`MAX_CONCURRENT_HANDSHAKES`] in
/// flight at once and yielding whichever completes first, so no connection is held up by a slow or
/// stalled peer.
pub struct Listener {
    listener: tokio::net::TcpListener,
    acceptor: Acceptor,
}

impl Listener {
    /// A listener over an already bound socket, using the acceptor returned by `acceptor` for each
    /// connection.
    pub fn new(
        listener: tokio::net::TcpListener,
        acceptor: impl Fn() -> TlsAcceptor + Send + 'static,
    ) -> Self {
        Self {
            listener,
            acceptor: Box::new(acceptor),
        }
    }

    async fn bind(socket: SocketAddr) -> Result<tokio::net::TcpListener, Error> {
        tokio::net::TcpListener::bind(socket)
            .await
            .map_err(|source| Error::Bind { socket, source })
    }

    /// Bind `socket` and serve reloadable file-backed TLS material, see [`TlsConfig::server_config`]
    /// and [`Reloadable`].
    pub async fn file(socket: SocketAddr, tls: &TlsConfig) -> Result<Self, Error> {
        let listener = Self::bind(socket).await?;
        let config = Reloadable::new(tls.clone(), "grpc-tls-cert-watcher", |tls| {
            tls.server_config()
        })?;
        Ok(Self::new(listener, move || TlsAcceptor::from(config.get())))
    }

    /// Bind `socket` and serve an in-memory TLS server configuration.
    pub async fn config(
        socket: SocketAddr,
        config: Arc<rustls::ServerConfig>,
    ) -> Result<Self, Error> {
        let listener = Self::bind(socket).await?;
        let acceptor = TlsAcceptor::from(config);
        Ok(Self::new(listener, move || acceptor.clone()))
    }

    /// Bind `socket` and serve an ephemeral self-signed certificate (auto-TLS), see
    /// [`auto_server_config`].
    pub async fn auto(socket: SocketAddr, subject_alt_names: Vec<String>) -> Result<Self, Error> {
        let config = Arc::new(auto_server_config(subject_alt_names)?);
        Self::config(socket, config).await
    }

    fn incoming<C, F>(
        self,
        accept: impl Fn(TcpStream, TlsAcceptor) -> F + Send + 'static,
    ) -> Pin<Box<dyn futures::Stream<Item = io::Result<C>> + Send>>
    where
        C: Send + 'static,
        F: std::future::Future<Output = io::Result<C>> + Send,
    {
        let acceptor = self.acceptor;
        TcpListenerStream::new(self.listener)
            .map(move |connection| {
                let acceptor = acceptor();
                let accept = connection.map(|connection| accept(connection, acceptor));
                async move {
                    match accept {
                        Ok(accept) => accept.await,
                        Err(error) => Err(error),
                    }
                }
            })
            .buffer_unordered(MAX_CONCURRENT_HANDSHAKES)
            .boxed()
    }

    /// Accept TLS connections only: plaintext clients fail the handshake and are rejected, so a
    /// configured TLS transport is never downgraded.
    pub fn tls_incoming(self) -> TlsIncoming {
        self.incoming(accept_tls)
    }

    /// Accept TLS and plaintext connections on the same port, by peeking the first byte of each
    /// (see [`MaybeTlsConnection`]).
    ///
    /// Only a server sitting at a mixed-version boundary (a plaintext peer and a TLS peer during a
    /// rolling upgrade) should use this; everything else, including such a server once TLS is
    /// enforced, must use [`Listener::tls_incoming`].
    pub fn sniffing_incoming(self) -> MaybeTlsIncoming {
        self.incoming(accept_sniffing)
    }
}

#[cfg(test)]
mod tests {
    use super::TlsConfig;
    use std::path::PathBuf;

    #[test]
    fn rejects_incomplete_identity() {
        assert!(TlsConfig::new(None, Some(PathBuf::from("client.pem")), None).is_err());
        assert!(TlsConfig::new(None, None, Some(PathBuf::from("client.key"))).is_err());
    }

    #[test]
    fn tracks_all_tls_files() {
        let tls = TlsConfig::new(
            Some(PathBuf::from("ca.pem")),
            Some(PathBuf::from("client.pem")),
            Some(PathBuf::from("client.key")),
        )
        .unwrap();

        assert!(tls.enabled());
        assert_eq!(
            tls.paths(),
            vec![
                PathBuf::from("ca.pem"),
                PathBuf::from("client.pem"),
                PathBuf::from("client.key"),
            ]
        );
    }
}
