// src/pairing/code.rs

use std::time::{Duration, Instant};

const CODE_LENGTH: usize = 8;
const CODE_CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const CODE_TTL: Duration = Duration::from_secs(120);

pub struct PairingCode {
    pub value: String,
    created_at: Instant,
}

impl PairingCode {
    pub fn generate() -> Result<Self, String> {
        let mut bytes = [0u8; CODE_LENGTH];
        getrandom::fill(&mut bytes)
            .map_err(|e| format!("failed to generate random bytes: {:?}", e))?;

        let value = bytes
            .iter()
            .map(|&b| CODE_CHARSET[(b as usize) % CODE_CHARSET.len()] as char)
            .collect();

        Ok(Self {
            value,
            created_at: Instant::now(),
        })
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > CODE_TTL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_length() {
        let code = PairingCode::generate().unwrap();
        assert_eq!(code.value.len(), CODE_LENGTH);
    }

    #[test]
    fn test_code_charset() {
        let code = PairingCode::generate().unwrap();
        assert!(code.value.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_code_not_expired_immediately() {
        let code = PairingCode::generate().unwrap();
        assert!(!code.is_expired());
    }

    #[test]
    fn test_codes_are_unique() {
        let codes: Vec<String> = (0..10)
            .map(|_| PairingCode::generate().unwrap().value)
            .collect();
        let unique: std::collections::HashSet<&String> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len());
    }
}
