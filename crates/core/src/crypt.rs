//! Optional password protection of a project's transcripts (open decision 7: off unless turned on).
//!
//! The password is stretched with Argon2id (64 MiB, 3 passes) into a 256-bit key, and each transcript is sealed with
//! XChaCha20-Poly1305 under a fresh random 192-bit nonce. Both come from the RustCrypto project (`argon2`,
//! `chacha20poly1305`). The project file keeps only the salt, the Argon2 settings and a sealed check value, so a
//! wrong password is told apart from a damaged file; the password itself is never stored.
//!
//! The key is held in memory while the project is unlocked (see [`remember`]) and wiped when it is locked again or
//! the program ends. There is no way back in without the password: a forgotten password means the transcripts are
//! lost, which the app says before protection is turned on.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// The first bytes of every sealed file.
pub const MAGIC: &[u8; 8] = b"PTSEAL1\n";
const CHECK: &[u8] = b"Prism Transcribe password check";

#[derive(Debug, thiserror::Error)]
pub enum CryptError {
    #[error("that password is not right")]
    WrongPassword,
    #[error("the file is damaged or was not sealed with this project's password")]
    Damaged,
    #[error("the password could not be used: {0}")]
    Kdf(String),
}

/// What the project file keeps about its password: never the password itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    /// Always "argon2id".
    pub kdf: String,
    pub memory_kib: u32,
    pub passes: u32,
    pub lanes: u32,
    /// Hex.
    pub salt: String,
    /// A known value sealed with the key (hex), to tell a wrong password from a damaged file.
    pub check: String,
}

/// A 256-bit key, wiped from memory when dropped.
pub struct Key(Zeroizing<[u8; 32]>);

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("the operating system's random number source");
    b
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn derive(password: &str, salt: &[u8], memory_kib: u32, passes: u32, lanes: u32) -> Result<Key, CryptError> {
    let params = Params::new(memory_kib, passes, lanes, Some(32)).map_err(|e| CryptError::Kdf(e.to_string()))?;
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(password.as_bytes(), salt, out.as_mut()).map_err(|e| CryptError::Kdf(e.to_string()))?;
    Ok(Key(out))
}

/// Settings for a new password. Tests use small ones so they run quickly.
pub(crate) fn new_lock_with(password: &str, memory_kib: u32, passes: u32) -> Result<(Lock, Key), CryptError> {
    let salt = random::<16>();
    let key = derive(password, &salt, memory_kib, passes, 1)?;
    let check = seal(&key, CHECK);
    Ok((Lock { kdf: "argon2id".into(), memory_kib, passes, lanes: 1, salt: hex(&salt), check: hex(&check) }, key))
}

/// A new password: a fresh salt and the key it gives.
pub fn new_lock(password: &str) -> Result<(Lock, Key), CryptError> {
    new_lock_with(password, 64 * 1024, 3)
}

/// The key for a project's password, or [`CryptError::WrongPassword`].
pub fn open_lock(lock: &Lock, password: &str) -> Result<Key, CryptError> {
    if lock.kdf != "argon2id" {
        return Err(CryptError::Kdf(format!("unknown method {}", lock.kdf)));
    }
    let salt = unhex(&lock.salt).ok_or(CryptError::Damaged)?;
    let key = derive(password, &salt, lock.memory_kib, lock.passes, lock.lanes)?;
    let check = unhex(&lock.check).ok_or(CryptError::Damaged)?;
    match unseal(&key, &check) {
        Ok(v) if v.as_slice() == CHECK => Ok(key),
        _ => Err(CryptError::WrongPassword),
    }
}

/// `MAGIC`, a random nonce, then the ciphertext with its 16-byte tag.
pub fn seal(key: &Key, plain: &[u8]) -> Vec<u8> {
    let nonce = random::<24>();
    let cipher = XChaCha20Poly1305::new(key.0.as_ref().into());
    let ct = cipher.encrypt(XNonce::from_slice(&nonce), plain).expect("sealing never fails for in-memory data");
    let mut out = Vec::with_capacity(MAGIC.len() + nonce.len() + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

pub fn is_sealed(data: &[u8]) -> bool {
    data.starts_with(MAGIC)
}

pub fn unseal(key: &Key, data: &[u8]) -> Result<Vec<u8>, CryptError> {
    let rest = data.strip_prefix(MAGIC.as_slice()).ok_or(CryptError::Damaged)?;
    if rest.len() < 24 + 16 {
        return Err(CryptError::Damaged);
    }
    let (nonce, ct) = rest.split_at(24);
    XChaCha20Poly1305::new(key.0.as_ref().into()).decrypt(XNonce::from_slice(nonce), ct).map_err(|_| CryptError::Damaged)
}

// ---------------------------------------------------------------------------------------------------------
// Unlocked projects
// ---------------------------------------------------------------------------------------------------------

static KEYS: Mutex<Option<HashMap<PathBuf, Arc<Key>>>> = Mutex::new(None);

fn canonical(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
}

/// Keep a project's key in memory until [`forget`] (or the program ends).
pub fn remember(dir: &Path, key: Key) {
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(canonical(dir), Arc::new(key));
}

/// Lock the project again: its key is dropped (and wiped once nothing is using it).
pub fn forget(dir: &Path) {
    if let Some(m) = KEYS.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        m.remove(&canonical(dir));
    }
}

pub fn key_for(dir: &Path) -> Option<Arc<Key>> {
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(&canonical(dir)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_unseal_with_the_right_password_only() {
        let (lock, key) = new_lock_with("correct horse", 256, 1).unwrap();
        let sealed = seal(&key, b"Maria said hello");
        assert!(is_sealed(&sealed));
        assert!(!sealed.windows(5).any(|w| w == b"Maria"), "nothing readable is left");
        let again = open_lock(&lock, "correct horse").unwrap();
        assert_eq!(unseal(&again, &sealed).unwrap(), b"Maria said hello");
        assert!(matches!(open_lock(&lock, "wrong horse"), Err(CryptError::WrongPassword)));
        // Two seals of the same text differ (fresh nonces), and a changed byte is caught.
        assert_ne!(seal(&key, b"x"), seal(&key, b"x"));
        let mut bad = sealed.clone();
        let n = bad.len();
        bad[n - 1] ^= 1;
        assert!(matches!(unseal(&key, &bad), Err(CryptError::Damaged)));
    }

    #[test]
    fn keys_are_remembered_per_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, key) = new_lock_with("pw", 256, 1).unwrap();
        assert!(key_for(tmp.path()).is_none());
        remember(tmp.path(), key);
        assert!(key_for(tmp.path()).is_some());
        forget(tmp.path());
        assert!(key_for(tmp.path()).is_none());
    }
}
