use std::{net::SocketAddr, sync::Arc};

use quinn::{
    ClientConfig, Endpoint, RecvStream, SendStream, ServerConfig,
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
};
use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

pub struct QuicStream {
    pub send: SendStream,
    pub recv: RecvStream,
}

/// Sender-side QUIC listener: bind first, get port, then accept.
pub struct QuicListener {
    endpoint: Endpoint,
}

impl QuicListener {
    /// Bind on the given port (use 0 for OS-assigned).
    pub fn bind(port: u16) -> Result<Self, String> {
        let (cert_der, key_der) = generate_cert()?;

        let server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .map_err(|e| format!("TLS server config error: {e}"))?;

        let server_config = ServerConfig::with_crypto(Arc::new(
            QuicServerConfig::try_from(server_crypto)
                .map_err(|e| format!("QUIC server config error: {e}"))?,
        ));

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let endpoint =
            Endpoint::server(server_config, addr).map_err(|e| format!("bind failed: {e}"))?;

        Ok(Self { endpoint })
    }

    /// Return the actual port the endpoint is listening on.
    pub fn port(&self) -> Result<u16, String> {
        self.endpoint
            .local_addr()
            .map(|a| a.port())
            .map_err(|e| format!("local_addr failed: {e}"))
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

        Ok(QuicStream { send, recv })
    }
}

/// Receiver: connect to sender's `SocketAddr` from DHT lookup
pub async fn connect(peer_addr: SocketAddr) -> Result<QuicStream, String> {
    let client_crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();

    let client_config = ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(client_crypto)
            .map_err(|e| format!("QUIC client config error: {e}"))?,
    ));

    let bind_addr = SocketAddr::from(([0, 0, 0, 0], 0));
    let mut endpoint =
        Endpoint::client(bind_addr).map_err(|e| format!("endpoint creation failed: {e}"))?;

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

    Ok(QuicStream { send, recv })
}

// --- internals ---

fn generate_cert() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), String> {
    let CertifiedKey { cert, signing_key } = generate_simple_self_signed(vec!["peer".to_string()])
        .map_err(|e| format!("cert generation failed: {e}"))?;

    let cert_der = cert.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));

    Ok((cert_der, key_der))
}

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
