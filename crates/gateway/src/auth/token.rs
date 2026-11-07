use rand_core::{OsRng, RngCore};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

pub const TOKEN_BYTES: usize = 32;
pub const SESSION_COOKIE: &str = "__Host-sid";

pub fn new_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn session_cookie(token: &str, max_age_secs: u64) -> String {
    format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age_secs}")
}

pub fn clear_cookie() -> String {
    format!("{SESSION_COOKIE}=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0")
}

pub fn read_cookie(header: &str) -> Option<&str> {
    header.split(';').map(str::trim).find_map(|part| part.strip_prefix(SESSION_COOKIE)?.strip_prefix('='))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_unique_and_hashed() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(token_hash(&a).len(), 64);
        assert_ne!(token_hash(&a), a);
    }

    #[test]
    fn constant_time_compare() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
    }

    #[test]
    fn cookie_round_trip() {
        let c = session_cookie("tok", 60);
        assert!(c.contains("HttpOnly") && c.contains("Secure") && c.contains("SameSite=Strict"));
        assert_eq!(read_cookie("a=1; __Host-sid=tok; b=2"), Some("tok"));
        assert_eq!(read_cookie("a=1"), None);
    }
}
