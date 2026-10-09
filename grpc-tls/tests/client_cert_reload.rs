//! End-to-end proof that a file-backed TLS client channel keeps the same `Channel` across a
//! certificate rotation while swapping the rustls `ClientConfig` under its connector.
//!
//! A real mutual-TLS server records the client leaf certificate presented on every handshake. A
//! single lazy client channel is built once with [`grpc_tls::file_tls_connect_lazy`]; after the
//! client certificate files are rotated on disk, the very same channel must start presenting the
//! new certificate (driven by the crate's own file watcher), with no rebuild of the channel.

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use grpc_tls::pki::{Certificate, CertificateAuthority};
use tokio::{io::AsyncReadExt, net::TcpListener};
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt;

/// The DER bytes of the leaf certificate in a PEM string.
fn leaf_der(pem: &str) -> Vec<u8> {
    rustls_pemfile::certs(&mut pem.as_bytes())
        .next()
        .expect("a certificate in the PEM")
        .expect("valid PEM certificate")
        .as_ref()
        .to_vec()
}

/// Write a client identity (certificate + key) to the given paths.
fn write_client(client: &Certificate, cert: &Path, key: &Path) {
    client.write(cert, key).expect("write client identity");
}

/// Run a mutual-TLS server that records the client leaf certificate of every accepted handshake.
///
/// The server only needs the TLS handshake to complete to observe the presented client
/// certificate, so it drops the connection immediately afterwards; the client's subsequent HTTP/2
/// attempt failing is irrelevant to what we assert.
async fn spawn_recording_server(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    observed: Arc<Mutex<Vec<Vec<u8>>>>,
) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let acceptor = acceptor.clone();
        let observed = observed.clone();
        tokio::spawn(async move {
            let Ok(mut tls) = acceptor.accept(stream).await else {
                return;
            };
            if let Some(certs) = tls.get_ref().1.peer_certificates() {
                if let Some(leaf) = certs.first() {
                    observed.lock().unwrap().push(leaf.as_ref().to_vec());
                }
            }
            // Drain briefly so the handshake flush completes, then drop to force the client to
            // reconnect (and thus re-run the connector) on its next request.
            let mut scratch = [0u8; 64];
            let _ = tls.read(&mut scratch).await;
        });
    }
}

/// Drive one request through the channel to force a connection attempt; the result is ignored
/// because the test server does not speak HTTP/2 (we only care that the TLS handshake ran).
async fn poke(channel: &tonic::transport::Channel) {
    let request = http::Request::builder()
        .uri("/")
        .body(tonic::body::Body::empty())
        .expect("build request");
    let _ = channel.clone().oneshot(request).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_channel_reloads_certificate_under_stable_channel() {
    let dir = std::env::temp_dir().join(format!("grpc-tls-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ca_path = dir.join("ca.pem");
    let server_crt = dir.join("server.crt");
    let server_key = dir.join("server.key");
    let client_crt = dir.join("client.crt");
    let client_key = dir.join("client.key");

    // A single CA that signs the server and both client identities, so the only variable across
    // the rotation is which client leaf is presented (the server trusts both).
    let ca = CertificateAuthority::generate("test-ca").unwrap();
    ca.write_certificate(&ca_path).unwrap();
    let server = ca
        .issue("server", &["127.0.0.1".to_string()])
        .expect("issue server identity");
    server.write(&server_crt, &server_key).unwrap();
    let client_a = ca.issue("client-a", &["client-a".to_string()]).unwrap();
    let client_b = ca.issue("client-b", &["client-b".to_string()]).unwrap();
    let der_a = leaf_der(client_a.certificate_pem());
    let der_b = leaf_der(client_b.certificate_pem());
    assert_ne!(der_a, der_b, "the two client leaves must differ");

    // Start with client-a on disk.
    write_client(&client_a, &client_crt, &client_key);

    // A mutual-TLS server: presents the server identity and requires a client certificate signed
    // by the CA.
    let server_config =
        grpc_tls::file_server_config(&server_crt, &server_key, Some(&ca_path)).unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let observed = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    tokio::spawn(spawn_recording_server(listener, acceptor, observed.clone()));

    // One lazy, file-backed TLS channel, built once and never rebuilt for the rest of the test.
    let tls = grpc_tls::TlsConfig::new(
        Some(ca_path.clone()),
        Some(client_crt.clone()),
        Some(client_key.clone()),
    )
    .unwrap();
    let endpoint =
        tonic::transport::Endpoint::from_shared(format!("http://127.0.0.1:{}", addr.port()))
            .unwrap()
            .connect_timeout(Duration::from_secs(2));
    let channel = grpc_tls::file_tls_connect_lazy(&endpoint, &tls).unwrap();

    // Before rotation: the channel must present client-a.
    assert!(
        wait_for_observed(&observed, &der_a, &channel).await,
        "server never observed the initial client-a certificate"
    );

    // Rotate the client identity on disk to client-b, in place, so the crate's file watcher reloads
    // it. A short pause ensures the modification time changes versus the initial write.
    tokio::time::sleep(Duration::from_millis(50)).await;
    write_client(&client_b, &client_crt, &client_key);

    // After rotation: the *same* channel must start presenting client-b, proving the connector
    // picked up the reloaded ClientConfig without the channel being rebuilt.
    assert!(
        wait_for_observed(&observed, &der_b, &channel).await,
        "server never observed the rotated client-b certificate on the same channel"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// Poke the channel until the server observes `target`, up to a generous timeout.
async fn wait_for_observed(
    observed: &Arc<Mutex<Vec<Vec<u8>>>>,
    target: &[u8],
    channel: &tonic::transport::Channel,
) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        poke(channel).await;
        if observed.lock().unwrap().iter().any(|der| der == target) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}
