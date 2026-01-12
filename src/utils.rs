use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rand::Rng;
use sha3::{Digest, Sha3_512};

/// Server password for salted hashing (generated once on first use)
static SERVER_PASSWORD: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// cyrb53 hash function - port from JavaScript
/// A fast and simple hash function with decent collision resistance.
/// (c) 2018 bryc (github.com/bryc) - Public domain
pub fn cyrb53(s: &str, seed: u32) -> u64 {
    let mut h1: u32 = 0xdeadbeef ^ seed;
    let mut h2: u32 = 0x41c6ce57 ^ seed;

    for ch in s.chars() {
        let ch = ch as u32;
        h1 = (h1 ^ ch).wrapping_mul(2654435761);
        h2 = (h2 ^ ch).wrapping_mul(1597334677);
    }

    h1 = (h1 ^ (h1 >> 16)).wrapping_mul(2246822507) ^ (h2 ^ (h2 >> 13)).wrapping_mul(3266489909);
    h2 = (h2 ^ (h2 >> 16)).wrapping_mul(2246822507) ^ (h1 ^ (h1 >> 13)).wrapping_mul(3266489909);

    (((h2 as u64) & 0x1fffff) << 32) | (h1 as u64)
}

/// Generate a random string of specified length
/// If letters_only is true, only uppercase A-Z are used
/// Otherwise, uses printable ASCII chars: -, /, 0-9, @, A-Z, a-z
pub fn get_random_string(length: usize, letters_only: bool) -> String {
    let mut rng = rand::thread_rng();
    let mut result = String::with_capacity(length);

    while result.len() < length {
        let r: u8 = rng.gen_range(0..128);

        let valid = if letters_only {
            // A-Z only
            (65..=90).contains(&r)
        } else {
            // Printable chars: - / 0-9 @ A-Z a-z
            r == 45 || (47..=57).contains(&r) || (64..=90).contains(&r) || (97..=122).contains(&r)
        };

        if valid {
            result.push(r as char);
        }
    }

    result
}

/// Generate a SHA3-512 salted hash
/// Uses a server-wide password that's generated on first call
pub fn hash_code_salted(salt: &str) -> String {
    let mut password_guard = SERVER_PASSWORD.lock();

    if password_guard.is_none() {
        *password_guard = Some(get_random_string(128, false));
    }

    let password = password_guard.as_ref().unwrap();

    // First hash the salt
    let mut salt_hasher = Sha3_512::new();
    salt_hasher.update(salt.as_bytes());
    let salt_hash = hex::encode(salt_hasher.finalize());

    // Then hash password + salt_hash
    let mut hasher = Sha3_512::new();
    hasher.update(password.as_bytes());
    hasher.update(salt_hash.as_bytes());

    hex::encode(hasher.finalize())
}

/// Validate UUID format
pub fn is_valid_uuid(uuid: &str) -> bool {
    // Pattern: 8-4-4-4-12 hex chars
    let parts: Vec<&str> = uuid.split('-').collect();
    if parts.len() != 5 {
        return false;
    }

    let expected_lens = [8, 4, 4, 4, 12];
    for (i, part) in parts.iter().enumerate() {
        if part.len() != expected_lens[i] {
            return false;
        }
        if !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cyrb53() {
        // Test consistency
        let hash1 = cyrb53("test", 0);
        let hash2 = cyrb53("test", 0);
        assert_eq!(hash1, hash2);

        // Different input should produce different output
        let hash3 = cyrb53("test2", 0);
        assert_ne!(hash1, hash3);

        // Different seed should produce different output
        let hash4 = cyrb53("test", 1);
        assert_ne!(hash1, hash4);
    }

    #[test]
    fn test_random_string() {
        let s1 = get_random_string(10, false);
        assert_eq!(s1.len(), 10);

        let s2 = get_random_string(256, true);
        assert_eq!(s2.len(), 256);
        assert!(s2.chars().all(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn test_is_valid_uuid() {
        assert!(is_valid_uuid("12345678-1234-1234-1234-123456789abc"));
        assert!(!is_valid_uuid("invalid"));
        assert!(!is_valid_uuid("12345678-1234-1234-1234-123456789ab")); // too short
        assert!(!is_valid_uuid("12345678-1234-1234-1234-123456789abcd")); // too long
    }
}
