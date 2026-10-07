use anyhow::{Context, Result, anyhow};
use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

/// Argon2id with the crate's default parameters; blocking, call via `spawn_blocking`.
pub fn hash(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow!("hashing password: {e}"))
}

pub fn verify(password: &str, hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(hash).map_err(|e| anyhow!("parsing stored hash: {e}"))?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(anyhow!("verifying password: {e}")),
    }
}

pub async fn hash_async(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || hash(&password))
        .await
        .context("hash task")?
}

pub async fn verify_async(password: String, hash: String) -> Result<bool> {
    tokio::task::spawn_blocking(move || verify(&password, &hash))
        .await
        .context("verify task")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify() {
        let h = hash("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify("correct horse battery", &h).unwrap());
        assert!(!verify("wrong password!", &h).unwrap());
    }

    #[test]
    fn hashes_are_salted() {
        assert_ne!(
            hash("same password 1").unwrap(),
            hash("same password 1").unwrap()
        );
    }

    #[test]
    fn garbage_hash_is_an_error() {
        assert!(verify("x", "not-a-hash").is_err());
    }
}
