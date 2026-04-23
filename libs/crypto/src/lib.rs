use bcrypt::{hash, verify, DEFAULT_COST};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("password hashing failed")]
    Hash(#[from] bcrypt::BcryptError),
}

pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn hash_password(password: &str) -> Result<String, CryptoError> {
    Ok(hash(password, DEFAULT_COST)?)
}

pub fn verify_password(password: &str, hashed: &str) -> Result<bool, CryptoError> {
    Ok(verify(password, hashed)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_is_stable() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hashes_and_verifies_password() {
        let hashed = hash_password("Aa@123456").expect("must hash");
        let ok = verify_password("Aa@123456", &hashed).expect("must verify");
        let bad = verify_password("wrong", &hashed).expect("must verify false");
        assert!(ok);
        assert!(!bad);
    }
}
