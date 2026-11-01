//! Key derivation for vault files.
//!
//! A vault is a single SQLCipher database. We use SQLCipher's *raw key* mode
//! and derive that key ourselves with Argon2id (much stronger against GPU
//! brute force than SQLCipher's built-in PBKDF2).
//!
//! The Argon2 salt doubles as the SQLCipher salt, which SQLCipher stores in
//! the first 16 bytes of the file. So the vault stays a single self-contained
//! file: read the salt from the header, derive the key, open.

use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::error::{Error, Result};

pub const SALT_LEN: usize = 16;
pub const KEY_LEN: usize = 32;

/// Argon2id parameters (v1): 64 MiB memory, 3 passes, 4 lanes.
/// Takes roughly 0.3-1 s on a modern desktop.
const M_COST_KIB: u32 = 64 * 1024;
const T_COST: u32 = 3;
const P_COST: u32 = 4;

pub fn random_salt() -> Result<[u8; SALT_LEN]> {
    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt).map_err(|e| Error::Crypto(e.to_string()))?;
    Ok(salt)
}

pub fn derive_key(password: &str, salt: &[u8; SALT_LEN]) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    derive_key_with(password, salt, M_COST_KIB, T_COST, P_COST)
}

fn derive_key_with(
    password: &str,
    salt: &[u8; SALT_LEN],
    m: u32,
    t: u32,
    p: u32,
) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    let params = Params::new(m, t, p, Some(KEY_LEN)).map_err(|e| Error::Crypto(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|e| Error::Crypto(e.to_string()))?;
    Ok(key)
}

/// Builds the SQLCipher raw-key pragma value: `x'<key hex><salt hex>'`.
pub fn sqlcipher_key_literal(key: &[u8; KEY_LEN], salt: &[u8; SALT_LEN]) -> Zeroizing<String> {
    let mut s = Zeroizing::new(String::with_capacity(3 + 2 * (KEY_LEN + SALT_LEN)));
    s.push_str("x'");
    s.push_str(&Zeroizing::new(hex::encode(key)));
    s.push_str(&hex::encode(salt));
    s.push('\'');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic_and_salted() {
        let salt_a = [1u8; SALT_LEN];
        let salt_b = [2u8; SALT_LEN];
        let k1 = derive_key_with("pw", &salt_a, 8, 1, 1).unwrap();
        let k2 = derive_key_with("pw", &salt_a, 8, 1, 1).unwrap();
        let k3 = derive_key_with("pw", &salt_b, 8, 1, 1).unwrap();
        assert_eq!(*k1, *k2);
        assert_ne!(*k1, *k3);
    }
}
