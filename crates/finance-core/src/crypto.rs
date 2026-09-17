//! Key derivation and key wrapping.
//!
//! Design (envelope encryption):
//! * A random 256-bit data key (DEK) encrypts the SQLCipher database.
//! * The DEK is never stored in the clear. It is stored twice, each copy
//!   encrypted ("wrapped") with AES-256-GCM under a key-encryption key (KEK):
//!   one KEK derived from the master password, one from the recovery key.
//! * KEKs are derived with Argon2id and a per-slot random salt.
//!
//! Changing the password only re-wraps the DEK; the database is untouched.

use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::Aes256Gcm;
use argon2::{Algorithm, Argon2, Params, Version};
use data_encoding::BASE64;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{CoreError, Result};

pub const KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
/// Associated data binds wrapped keys to this format version.
const AAD: &[u8] = b"finance-app/vault/v1";

pub type SecretKey = Zeroizing<[u8; KEY_LEN]>;

/// Argon2id cost parameters, stored alongside each slot so they can be raised
/// in future versions without breaking existing vaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Memory in KiB.
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl KdfParams {
    /// Production defaults: 256 MiB, 3 passes, 1 lane (roughly 0.5-1 s on an
    /// Apple-silicon Mac). Well above OWASP's minimum for Argon2id.
    pub const DEFAULT: KdfParams = KdfParams { m_cost_kib: 256 * 1024, t_cost: 3, p_cost: 1 };

    /// Cheap parameters for unit tests only.
    #[cfg(test)]
    pub const TEST: KdfParams = KdfParams { m_cost_kib: 1024, t_cost: 1, p_cost: 1 };

    fn validate(&self) -> Result<()> {
        // Refuse suspiciously weak parameters read from disk, so a tampered
        // vault.json cannot silently downgrade the KDF.
        #[cfg(not(test))]
        if self.m_cost_kib < 64 * 1024 || self.t_cost < 2 {
            return Err(CoreError::CorruptVault);
        }
        if self.p_cost == 0 || self.p_cost > 16 {
            return Err(CoreError::CorruptVault);
        }
        Ok(())
    }
}

/// One wrapped copy of the data key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeySlot {
    pub kdf: KdfParams,
    pub salt: String,
    pub nonce: String,
    pub wrapped_key: String,
}

pub fn random_key() -> SecretKey {
    let mut k = Zeroizing::new([0u8; KEY_LEN]);
    OsRng.fill_bytes(k.as_mut());
    k
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    OsRng.fill_bytes(&mut b);
    b
}

pub fn derive_kek(secret: &[u8], salt: &[u8], params: KdfParams) -> Result<SecretKey> {
    params.validate()?;
    let p = Params::new(params.m_cost_kib, params.t_cost, params.p_cost, Some(KEY_LEN))
        .map_err(|_| CoreError::Crypto)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, p);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    argon.hash_password_into(secret, salt, out.as_mut()).map_err(|_| CoreError::Crypto)?;
    Ok(out)
}

/// Wrap `dek` under a key derived from `secret`.
pub fn seal_slot(secret: &[u8], dek: &SecretKey, params: KdfParams) -> Result<KeySlot> {
    let salt: [u8; SALT_LEN] = random_bytes();
    let nonce: [u8; NONCE_LEN] = random_bytes();
    let kek = derive_kek(secret, &salt, params)?;
    let cipher = Aes256Gcm::new(kek.as_ref().into());
    let mut ct = dek.to_vec();
    cipher
        .encrypt_in_place(&nonce.into(), AAD, &mut ct)
        .map_err(|_| CoreError::Crypto)?;
    Ok(KeySlot {
        kdf: params,
        salt: BASE64.encode(&salt),
        nonce: BASE64.encode(&nonce),
        wrapped_key: BASE64.encode(&ct),
    })
}

/// Unwrap the data key. Any failure (wrong secret or tampered slot) yields
/// `WrongPassword`: AES-GCM authentication cannot tell the two apart.
pub fn open_slot(secret: &[u8], slot: &KeySlot) -> Result<SecretKey> {
    let salt = BASE64.decode(slot.salt.as_bytes()).map_err(|_| CoreError::CorruptVault)?;
    let nonce = BASE64.decode(slot.nonce.as_bytes()).map_err(|_| CoreError::CorruptVault)?;
    let ct = BASE64.decode(slot.wrapped_key.as_bytes()).map_err(|_| CoreError::CorruptVault)?;
    if salt.len() != SALT_LEN || nonce.len() != NONCE_LEN {
        return Err(CoreError::CorruptVault);
    }
    let kek = derive_kek(secret, &salt, slot.kdf)?;
    let cipher = Aes256Gcm::new(kek.as_ref().into());
    let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| CoreError::CorruptVault)?;
    let mut pt = Zeroizing::new(ct);
    cipher
        .decrypt_in_place(&nonce.into(), AAD, &mut *pt)
        .map_err(|_| CoreError::WrongPassword)?;
    if pt.len() != KEY_LEN {
        return Err(CoreError::CorruptVault);
    }
    let mut dek = Zeroizing::new([0u8; KEY_LEN]);
    dek.copy_from_slice(&pt);
    Ok(dek)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_then_open_round_trips() {
        let dek = random_key();
        let slot = seal_slot(b"correct horse battery", &dek, KdfParams::TEST).unwrap();
        let out = open_slot(b"correct horse battery", &slot).unwrap();
        assert_eq!(*out, *dek);
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let dek = random_key();
        let slot = seal_slot(b"correct horse battery", &dek, KdfParams::TEST).unwrap();
        assert!(matches!(open_slot(b"wrong", &slot), Err(CoreError::WrongPassword)));
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let dek = random_key();
        let mut slot = seal_slot(b"pw", &dek, KdfParams::TEST).unwrap();
        let mut ct = BASE64.decode(slot.wrapped_key.as_bytes()).unwrap();
        ct[0] ^= 1;
        slot.wrapped_key = BASE64.encode(&ct);
        assert!(open_slot(b"pw", &slot).is_err());
    }

    #[test]
    fn salts_are_unique_per_slot() {
        let dek = random_key();
        let a = seal_slot(b"pw", &dek, KdfParams::TEST).unwrap();
        let b = seal_slot(b"pw", &dek, KdfParams::TEST).unwrap();
        assert_ne!(a.salt, b.salt);
        assert_ne!(a.wrapped_key, b.wrapped_key);
    }
}

#[cfg(test)]
mod timing {
    use super::*;

    /// Run with: cargo test --release -p finance-core kdf_timing -- --ignored --nocapture
    #[test]
    #[ignore]
    fn kdf_timing() {
        let start = std::time::Instant::now();
        derive_kek(b"benchmark password", &[7u8; SALT_LEN], KdfParams::DEFAULT).unwrap();
        println!("Argon2id with default params took {:?}", start.elapsed());
    }
}
