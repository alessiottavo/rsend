use std::sync::Arc;
use std::{net::SocketAddr, net::UdpSocket};

use quinn::{
    ClientConfig, Endpoint, EndpointConfig, RecvStream, SendStream, ServerConfig,
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
};
use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

pub struct QuicStream {
    pub send: SendStream,
    pub recv: RecvStream,
    /// Keep the endpoint alive so Quinn continues driving UDP I/O.
    _endpoint: Endpoint,
}

/// Sender-side QUIC listener built from a pre-bound UDP socket.
pub struct QuicListener {
    endpoint: Endpoint,
}

impl QuicListener {
    /// Create a QUIC server endpoint from an existing UDP socket.
    ///
    /// The socket must already be bound and set to non-blocking mode.
    pub fn from_socket(udp: UdpSocket) -> Result<Self, String> {
        let server_config = make_server_config()?;

        let endpoint = Endpoint::new(
            EndpointConfig::default(),
            Some(server_config),
            udp,
            Arc::new(quinn::TokioRuntime),
        )
        .map_err(|e| format!("QUIC endpoint creation failed: {e}"))?;

        Ok(Self { endpoint })
    }

    /// Accept one incoming connection and return its bidirectional stream.
    pub async fn accept(self) -> Result<QuicStream, String> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or("endpoint closed before connection")?;

        let conn = incoming
            .await
            .map_err(|e| format!("handshake failed: {e}"))?;

        let (send, recv) = conn
            .accept_bi()
            .await
            .map_err(|e| format!("stream accept failed: {e}"))?;

        Ok(QuicStream {
            send,
            recv,
            _endpoint: self.endpoint,
        })
    }
}

/// Receiver: connect to sender's address using a pre-bound UDP socket.
pub async fn connect(udp: UdpSocket, peer_addr: SocketAddr) -> Result<QuicStream, String> {
    let client_config = make_client_config()?;

    let mut endpoint = Endpoint::new(
        EndpointConfig::default(),
        None,
        udp,
        Arc::new(quinn::TokioRuntime),
    )
    .map_err(|e| format!("QUIC endpoint creation failed: {e}"))?;

    endpoint.set_default_client_config(client_config);

    let conn = endpoint
        .connect(peer_addr, "peer")
        .map_err(|e| format!("connect error: {e}"))?
        .await
        .map_err(|e| format!("handshake failed: {e}"))?;

    let (send, recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("stream open failed: {e}"))?;

    Ok(QuicStream {
        send,
        recv,
        _endpoint: endpoint,
    })
}

// --- TLS config helpers ---

fn make_server_config() -> Result<ServerConfig, String> {
    let (cert_der, key_der) = generate_cert()?;

    let server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .map_err(|e| format!("TLS server config error: {e}"))?;

    ServerConfig::with_crypto(Arc::new(
        QuicServerConfig::try_from(server_crypto)
            .map_err(|e| format!("QUIC server config error: {e}"))?,
    ))
    .pipe(Ok)
}

fn make_client_config() -> Result<ClientConfig, String> {
    let client_crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();

    ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(client_crypto)
            .map_err(|e| format!("QUIC client config error: {e}"))?,
    ))
    .pipe(Ok)
}

fn generate_cert() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), String> {
    let CertifiedKey { cert, signing_key } = generate_simple_self_signed(vec!["peer".to_string()])
        .map_err(|e| format!("cert generation failed: {e}"))?;

    let cert_der = cert.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));

    Ok((cert_der, key_der))
}

/// Pipe helper — allows `expr.pipe(Ok)` to wrap in Result without nesting.
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

impl<T> Pipe for T {}

/// No-op TLS verifier — pairing code is the trust anchor, not the cert chain.
#[derive(Debug)]
struct NoVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _server_name: &rustls::pki_types::ServerName,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dsa: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dsa,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dsa: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dsa,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify QUIC stream can be established over loopback with pre-bound sockets.
    #[tokio::test]
    async fn test_quic_from_socket_loopback() {
        let server_udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        server_udp.set_nonblocking(true).unwrap();
        let server_addr = server_udp.local_addr().unwrap();

        let client_udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        client_udp.set_nonblocking(true).unwrap();

        let listener = QuicListener::from_socket(server_udp).unwrap();

        // Server: accept connection, read data from the stream
        let accept_handle = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            let data = stream.recv.read_to_end(1024).await.unwrap();
            data
        });

        // Client: connect, write data (triggers the stream creation on the wire),
        // then return the stream to keep the endpoint alive until the server reads.
        let connect_handle = tokio::spawn(async move {
            let mut stream = connect(client_udp, server_addr).await.unwrap();
            stream.send.write_all(b"hello").await.unwrap();
            stream.send.finish().unwrap();
            stream // keep alive
        });

        let (server_result, client_result) = tokio::join!(accept_handle, connect_handle);
        let _client_stream = client_result.unwrap();
        let data = server_result.unwrap();

        assert_eq!(data, b"hello");
    }
}
