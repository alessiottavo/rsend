const SENDER_KEY_CONTEXT: &str = "rsend dht sender v1";
const RECEIVER_KEY_CONTEXT: &str = "rsend dht receiver v1";
const ALIAS_CONTEXT: &str = "rsend alias v1";

pub fn derive_sender_key(pairing_code: &str) -> [u8; 32] {
    blake3::derive_key(SENDER_KEY_CONTEXT, pairing_code.as_bytes())
}

pub fn derive_receiver_key(pairing_code: &str) -> [u8; 32] {
    blake3::derive_key(RECEIVER_KEY_CONTEXT, pairing_code.as_bytes())
}

pub fn derive_alias_bytes(session_key: &[u8]) -> [u8; 32] {
    blake3::derive_key(ALIAS_CONTEXT, session_key)
}

pub fn hash_file(content: &[u8]) -> [u8; 32] {
    blake3::hash(content).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sender_key_is_deterministic() {
        let key1 = derive_sender_key("ab3def12");
        let key2 = derive_sender_key("ab3def12");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_receiver_key_is_deterministic() {
        let key1 = derive_receiver_key("ab3def12");
        let key2 = derive_receiver_key("ab3def12");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_sender_key_is_32_bytes() {
        assert_eq!(derive_sender_key("ab3def12").len(), 32);
    }

    #[test]
    fn test_receiver_key_is_32_bytes() {
        assert_eq!(derive_receiver_key("ab3def12").len(), 32);
    }

    #[test]
    fn test_sender_and_receiver_keys_differ_for_same_code() {
        let sender = derive_sender_key("ab3def12");
        let receiver = derive_receiver_key("ab3def12");
        assert_ne!(sender, receiver);
    }

    #[test]
    fn test_different_codes_produce_different_sender_keys() {
        let key1 = derive_sender_key("ab3def12");
        let key2 = derive_sender_key("ab3def13");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_different_codes_produce_different_receiver_keys() {
        let key1 = derive_receiver_key("ab3def12");
        let key2 = derive_receiver_key("ab3def13");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_alias_bytes_deterministic() {
        let session_key = [0u8; 32];
        let alias1 = derive_alias_bytes(&session_key);
        let alias2 = derive_alias_bytes(&session_key);
        assert_eq!(alias1, alias2);
    }

    #[test]
    fn test_alias_bytes_different_keys_produce_different_aliases() {
        let key1 = [0u8; 32];
        let key2 = [1u8; 32];
        assert_ne!(derive_alias_bytes(&key1), derive_alias_bytes(&key2));
    }

    #[test]
    fn test_file_hash_deterministic() {
        let content = b"hello world";
        assert_eq!(hash_file(content), hash_file(content));
    }

    #[test]
    fn test_file_hash_different_content() {
        assert_ne!(hash_file(b"hello"), hash_file(b"hello!"));
    }
}
