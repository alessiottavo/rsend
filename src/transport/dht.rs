use mainline::Dht;
use std::net::SocketAddr;

pub fn announce(dht_key: &[u8; 32], port: u16) -> Result<(), String> {
    let dht = Dht::client().map_err(|e| format!("failed to create DHT client: {e}"))?;

    // convert our 32-byte BLAKE3 key to a 20-byte infohash
    // DHT uses SHA1-sized (20 byte) keys — take first 20 bytes
    let mut infohash = [0u8; 20];
    infohash.copy_from_slice(&dht_key[..20]);

    dht.announce_peer(infohash.into(), Some(port))
        .map_err(|e| format!("failed to announce on DHT: {e}"))?;

    Ok(())
}

pub fn lookup(dht_key: &[u8; 32]) -> Result<SocketAddr, String> {
    let dht = Dht::client().map_err(|e| format!("failed to create DHT client: {e}"))?;

    let mut infohash = [0u8; 20];
    infohash.copy_from_slice(&dht_key[..20]);

    let peers = dht.get_peers(infohash.into());

    peers
        .flat_map(IntoIterator::into_iter)
        .next()
        .map(SocketAddr::from)
        .ok_or_else(|| "no peer found for this pairing code".to_string())
}
