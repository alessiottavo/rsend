use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

const STUN_SERVER_1: &str = "stun.l.google.com:19302";
const STUN_SERVER_2: &str = "stun1.l.google.com:19302";

/// Detected NAT type based on port mapping consistency across STUN servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    /// Same external port for different destinations — hole-punching works.
    Cone,
    /// Different external port per destination — hole-punching will likely fail.
    Symmetric,
}

/// Bind a UDP socket and discover our public address via STUN.
///
/// Queries two STUN servers on the same socket to detect NAT type:
/// - If both return the same port → `Cone` (hole-punch friendly)
/// - If ports differ → `Symmetric` (CGNAT, hole-punch won't work)
///
/// Falls back to `Cone` if the second STUN query fails.
pub async fn bind_and_discover() -> Result<(tokio::net::UdpSocket, SocketAddr, NatType), String> {
    let std_socket =
        std::net::UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("bind UDP: {e}"))?;
    std_socket
        .set_nonblocking(true)
        .map_err(|e| format!("set nonblocking: {e}"))?;

    let stun_clone = std_socket
        .try_clone()
        .map_err(|e| format!("clone socket for STUN: {e}"))?;

    let (public_addr, nat_type) = tokio::task::spawn_blocking(move || {
        async_std::task::block_on(async {
            stun_clone
                .set_nonblocking(false)
                .map_err(|e| format!("set blocking for STUN: {e}"))?;

            let async_socket = async_std::net::UdpSocket::from(stun_clone);
            let mut client =
                stun_client::Client::from_socket(Arc::new(async_socket), None);

            // First STUN query — required for public address
            let msg1 = client
                .binding_request(STUN_SERVER_1, None)
                .await
                .map_err(|e| format!("STUN binding request failed: {e}"))?;

            let addr1 = stun_client::Attribute::get_xor_mapped_address(&msg1)
                .ok_or_else(|| "STUN response missing XOR-MAPPED-ADDRESS".to_string())?;

            // Second STUN query — for NAT type detection
            let nat_type = match client.binding_request(STUN_SERVER_2, None).await {
                Ok(msg2) => {
                    match stun_client::Attribute::get_xor_mapped_address(&msg2) {
                        Some(addr2) if addr1.port() == addr2.port() => NatType::Cone,
                        Some(_) => NatType::Symmetric,
                        None => NatType::Cone, // missing attribute — optimistic
                    }
                }
                Err(_) => NatType::Cone, // second query failed — optimistic
            };

            Ok::<_, String>((addr1, nat_type))
        })
    })
    .await
    .map_err(|e| format!("STUN task panicked: {e}"))??;

    let tokio_socket = tokio::net::UdpSocket::from_std(std_socket)
        .map_err(|e| format!("wrap socket in tokio: {e}"))?;

    Ok((tokio_socket, public_addr, nat_type))
}

/// Spawn a background task that continuously punches toward `peer`.
///
/// Sends a 1-byte packet every 100ms for up to 30s (matches Quinn's default
/// idle timeout). Returns a `JoinHandle` that should be aborted once the QUIC
/// handshake succeeds or fails.
pub fn punch_background(
    socket: std::net::UdpSocket,
    peer: SocketAddr,
) -> Result<tokio::task::JoinHandle<()>, String> {
    let tokio_socket =
        tokio::net::UdpSocket::from_std(socket).map_err(|e| format!("wrap clone in tokio: {e}"))?;

    Ok(tokio::spawn(async move {
        for _ in 0..300 {
            // Best-effort — ignore errors
            let _ = tokio_socket.send_to(&[0u8], peer).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }))
}

/// Send UDP hole-punch packets to a peer address.
///
/// Each packet is a single zero byte — just enough to create a NAT mapping.
/// The peer sends the same packets back, so both NATs open a pinhole.
pub async fn punch_hole(
    udp: &tokio::net::UdpSocket,
    peer: SocketAddr,
    packets: u32,
    interval: Duration,
) -> Result<(), String> {
    for i in 0..packets {
        udp.send_to(&[0u8], peer)
            .await
            .map_err(|e| format!("hole-punch packet {i}: {e}"))?;

        if i + 1 < packets {
            tokio::time::sleep(interval).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_punch_hole_sends_expected_packets() {
        let sender = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .unwrap();
        let receiver = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .unwrap();
        let receiver_addr = receiver.local_addr().unwrap();

        let packet_count = 5;
        punch_hole(&sender, receiver_addr, packet_count, Duration::from_millis(1))
            .await
            .unwrap();

        // Give packets a moment to arrive
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut buf = [0u8; 16];
        let mut count = 0u32;

        // Non-blocking reads to count received packets
        loop {
            match receiver.try_recv_from(&mut buf) {
                Ok((len, _)) => {
                    assert_eq!(len, 1);
                    assert_eq!(buf[0], 0);
                    count += 1;
                }
                Err(_) => break,
            }
        }

        assert_eq!(count, packet_count);
    }

    #[tokio::test]
    async fn test_punch_hole_zero_packets() {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .unwrap();
        let target: SocketAddr = "127.0.0.1:9999".parse().unwrap();

        // Should succeed without sending anything
        punch_hole(&socket, target, 0, Duration::from_millis(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_punch_background_sends_packets_until_abort() {
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.set_nonblocking(true).unwrap();

        let receiver = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .unwrap();
        let receiver_addr = receiver.local_addr().unwrap();

        let handle = punch_background(sender, receiver_addr).unwrap();

        // Let it run for a bit
        tokio::time::sleep(Duration::from_millis(350)).await;
        handle.abort();

        // Drain received packets
        let mut buf = [0u8; 16];
        let mut count = 0u32;
        loop {
            match receiver.try_recv_from(&mut buf) {
                Ok((len, _)) => {
                    assert_eq!(len, 1);
                    assert_eq!(buf[0], 0);
                    count += 1;
                }
                Err(_) => break,
            }
        }

        // At 100ms intervals, ~350ms should yield 3-4 packets (timing slack)
        assert!(count >= 2, "expected at least 2 packets, got {count}");
        assert!(count <= 10, "expected at most 10 packets, got {count}");
    }

    /// STUN requires internet access — run with `cargo test -- --ignored`
    #[tokio::test]
    #[ignore = "requires internet access"]
    async fn test_bind_and_discover_returns_public_addr() {
        let (_socket, addr, nat_type) = bind_and_discover().await.unwrap();
        // Public address should not be unspecified or loopback
        assert!(!addr.ip().is_unspecified());
        assert!(!addr.ip().is_loopback());
        assert_ne!(addr.port(), 0);
        // NAT type should be one of the two variants
        assert!(nat_type == NatType::Cone || nat_type == NatType::Symmetric);
    }
}
