use mainline::Dht;
use std::net::SocketAddr;
use std::time::Duration;

pub async fn announce(dht_key: &[u8; 32], port: u16) -> Result<(), String> {
    let infohash = to_infohash(dht_key);

    tokio::task::spawn_blocking(move || {
        let dht = Dht::client().map_err(|e| format!("failed to create DHT client: {e}"))?;
        dht.announce_peer(infohash.into(), Some(port))
            .map_err(|e| format!("failed to announce on DHT: {e}"))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("DHT announce task panicked: {e}"))?
}

pub async fn lookup(dht_key: &[u8; 32]) -> Result<SocketAddr, String> {
    let infohash = to_infohash(dht_key);

    tokio::task::spawn_blocking(move || {
        let dht = Dht::client().map_err(|e| format!("failed to create DHT client: {e}"))?;
        let peers = dht.get_peers(infohash.into());

        peers
            .flat_map(IntoIterator::into_iter)
            .next()
            .map(SocketAddr::from)
            .ok_or_else(|| "no peer found for this pairing code".to_string())
    })
    .await
    .map_err(|e| format!("DHT lookup task panicked: {e}"))?
}

pub async fn lookup_with_retry(
    dht_key: &[u8; 32],
    max_attempts: u32,
    delay: Duration,
) -> Result<SocketAddr, String> {
    retry(max_attempts, delay, || lookup(dht_key)).await
}

/// Retry an async operation up to `max_attempts` times with `delay` between attempts.
/// Returns the first `Ok` result, or the last `Err` if all attempts fail.
async fn retry<F, Fut, T>(max_attempts: u32, delay: Duration, f: F) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let mut last_err = String::new();

    for attempt in 1..=max_attempts {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                last_err = e;
                if attempt < max_attempts {
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    Err(last_err)
}

/// Convert a 32-byte BLAKE3 key to a 20-byte infohash (DHT uses SHA1-sized keys).
fn to_infohash(dht_key: &[u8; 32]) -> [u8; 20] {
    let mut infohash = [0u8; 20];
    infohash.copy_from_slice(&dht_key[..20]);
    infohash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_infohash_takes_first_20_bytes() {
        let key = [0xABu8; 32];
        let infohash = to_infohash(&key);
        assert_eq!(infohash.len(), 20);
        assert!(infohash.iter().all(|&b| b == 0xAB));
    }

    #[tokio::test]
    async fn test_retry_succeeds_on_first_attempt() {
        let result = retry(3, Duration::from_millis(1), || async { Ok::<_, String>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_succeeds_on_later_attempt() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = counter.clone();

        let result = retry(3, Duration::from_millis(1), move || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 2 {
                    Err("not yet".to_string())
                } else {
                    Ok(99)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 99);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausts_all_attempts() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = counter.clone();

        let result: Result<(), String> = retry(3, Duration::from_millis(1), move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err("always fails".to_string())
            }
        })
        .await;

        assert_eq!(result.unwrap_err(), "always fails");
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 3);
    }
}
