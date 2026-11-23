pub mod rate_limit;

use std::sync::Arc;

use argon2::password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;

use crate::error::{AppError, AppResult};

// Pre-computed dummy Argon2id hash so failed logins for unknown emails still run
// a full Argon2id verification pass (timing-attack mitigation).
const DUMMY_ARGON2_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// Bounded password hashing service using Argon2id (19 MiB, t=2, p=1).
/// Caps concurrent hashes via a semaphore to protect 1-GB VPS memory.
#[derive(Clone)]
pub struct PasswordService {
    semaphore: Arc<Semaphore>,
}

impl Default for PasswordService {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordService {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(2)),
        }
    }

    fn hasher() -> Argon2<'static> {
        let params = Params::new(19_456, 2, 1, None).expect("valid Argon2id params");
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    }

    pub async fn hash_password(&self, password: &str) -> AppResult<String> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AppError::Internal("password semaphore closed".into()))?;
        let owned = password.to_owned();
        tokio::task::spawn_blocking(move || {
            Self::hasher()
                .hash_password(owned.as_bytes())
                .map(|h| h.to_string())
                .map_err(|e| AppError::Internal(format!("argon2 hash failure: {e}")))
        })
        .await
        .map_err(|e| AppError::Internal(format!("argon2 task join failure: {e}")))?
    }

    pub async fn verify_password(
        &self,
        password: &str,
        stored_hash: Option<&str>,
    ) -> AppResult<bool> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AppError::Internal("password semaphore closed".into()))?;
        let owned_pw = password.to_owned();
        let owned_hash = stored_hash.unwrap_or(DUMMY_ARGON2_HASH).to_owned();
        let is_real_user = stored_hash.is_some();

        tokio::task::spawn_blocking(move || {
            let Ok(parsed) = PasswordHash::new(&owned_hash) else {
                return Ok(false);
            };
            let ok = Self::hasher()
                .verify_password(owned_pw.as_bytes(), &parsed)
                .is_ok();
            Ok(is_real_user && ok)
        })
        .await
        .map_err(|e| AppError::Internal(format!("argon2 verify task join failure: {e}")))?
    }
}

/// Generates a 32-byte cryptographically secure random token encoded as URL-safe base64.
pub fn generate_token_32() -> AppResult<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| AppError::Internal(format!("rng failed: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    ab.len() == bb.len() && bool::from(ab.ct_eq(bb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_and_verify_argon2id() {
        let svc = PasswordService::new();
        let hash = svc.hash_password("S3nhaForte!123").await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(
            svc.verify_password("S3nhaForte!123", Some(&hash))
                .await
                .unwrap()
        );
        assert!(
            !svc.verify_password("wrong-password", Some(&hash))
                .await
                .unwrap()
        );
        assert!(!svc.verify_password("S3nhaForte!123", None).await.unwrap());
    }

    #[test]
    fn tokens_and_constant_time() {
        let t1 = generate_token_32().unwrap();
        let t2 = generate_token_32().unwrap();
        assert_ne!(t1, t2);
        assert!(t1.len() >= 43);
        assert!(constant_time_eq(&t1, &t1));
        assert!(!constant_time_eq(&t1, &t2));
    }
}
