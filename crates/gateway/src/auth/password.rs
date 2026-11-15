use rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

use crate::error::GatewayError;

pub const MIN_PASSWORD_CHARS: usize = 12;
pub const MIN_SIGNUP_PASSWORD_CHARS: usize = 8;

#[derive(Debug, Clone, Copy)]
pub struct HashCost {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for HashCost {
    fn default() -> Self {
        Self { memory_kib: 64 * 1024, iterations: 3, parallelism: 1 }
    }
}

fn hasher(cost: HashCost) -> Result<Argon2<'static>, GatewayError> {
    let params = Params::new(cost.memory_kib, cost.iterations, cost.parallelism, None).map_err(|_| GatewayError::Internal)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

pub fn check_policy(password: &str) -> Result<(), GatewayError> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(GatewayError::WeakPassword);
    }
    Ok(())
}

pub fn check_signup_policy(password: &str) -> Result<(), GatewayError> {
    if password.chars().count() < MIN_SIGNUP_PASSWORD_CHARS {
        return Err(GatewayError::Invalid("비밀번호는 8자 이상이어야 합니다"));
    }
    Ok(())
}

fn hash(password: &str, cost: HashCost) -> Result<String, GatewayError> {
    let salt = SaltString::generate(&mut OsRng);
    hasher(cost)?
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| GatewayError::Internal)
}

pub fn hash_password(password: &str, cost: HashCost) -> Result<String, GatewayError> {
    check_policy(password)?;
    hash(password, cost)
}

pub fn hash_signup_password(password: &str, cost: HashCost) -> Result<String, GatewayError> {
    check_signup_policy(password)?;
    hash(password, cost)
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cheap() -> HashCost {
        HashCost { memory_kib: 8 * 1024, iterations: 1, parallelism: 1 }
    }

    #[test]
    fn hash_then_verify() -> Result<(), GatewayError> {
        let stored = hash_password("correct horse battery", cheap())?;
        assert!(stored.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &stored));
        assert!(!verify_password("correct horse batterY", &stored));
        Ok(())
    }

    #[test]
    fn short_password_is_refused() {
        assert!(matches!(hash_password("short", cheap()), Err(GatewayError::WeakPassword)));
    }

    #[test]
    fn signup_accepts_eight_chars_and_refuses_seven() -> Result<(), GatewayError> {
        let stored = hash_signup_password("eight888", cheap())?;
        assert!(verify_password("eight888", &stored));
        assert!(matches!(hash_signup_password("seven77", cheap()), Err(GatewayError::Invalid(_))));
        Ok(())
    }

    #[test]
    fn broken_hash_never_verifies() {
        assert!(!verify_password("anything at all", "not-a-hash"));
    }
}
