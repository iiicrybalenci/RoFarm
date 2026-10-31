use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{VaultError, VaultResult};

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;
pub const SALT_LEN: usize = 32;

pub type SecretKey = Zeroizing<[u8; KEY_LEN]>;

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    rand::thread_rng().fill_bytes(&mut out);
    out
}

pub fn random_key() -> SecretKey {
    Zeroizing::new(random_bytes::<KEY_LEN>())
}

pub fn random_nonce() -> [u8; NONCE_LEN] {
    random_bytes::<NONCE_LEN>()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    pub nonce: String,
    pub ciphertext: String,
}

impl Sealed {
    pub fn nonce_bytes(&self) -> VaultResult<[u8; NONCE_LEN]> {
        let bytes = b64_decode(&self.nonce).map_err(|_| VaultError::Corrupt)?;
        bytes.try_into().map_err(|_| VaultError::Corrupt)
    }
}

pub fn seal(key: &[u8; KEY_LEN], aad: &[u8], plaintext: &[u8]) -> VaultResult<Sealed> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = random_nonce();
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad })
        .map_err(|_| VaultError::Crypto)?;
    Ok(Sealed { nonce: b64_encode(&nonce), ciphertext: b64_encode(&ciphertext) })
}

pub fn open(
    key: &[u8; KEY_LEN],
    aad: &[u8],
    sealed: &Sealed,
) -> VaultResult<Zeroizing<Vec<u8>>> {
    let nonce = sealed.nonce_bytes()?;
    let ciphertext = b64_decode(&sealed.ciphertext).map_err(|_| VaultError::Corrupt)?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let plaintext = cipher
        .decrypt(XNonce::from_slice(&nonce), Payload { msg: &ciphertext, aad })
        .map_err(|_| VaultError::AuthenticationFailed)?;
    Ok(Zeroizing::new(plaintext))
}

pub fn keyed_hash(key: &[u8; KEY_LEN], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(key);
    for part in parts {
        hasher.update(part);
        hasher.update(&[0u8]);
    }
    *hasher.finalize().as_bytes()
}

pub fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part);
        hasher.update(&[0u8]);
    }
    *hasher.finalize().as_bytes()
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn b64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes)
}

pub fn b64_decode(text: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD_NO_PAD.decode(text)
}

pub mod b64 {
    use super::{b64_decode, b64_encode};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&b64_encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(deserializer)?;
        b64_decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn seal_open_round_trip_and_rejects_tamper() {
        let key = random_key();
        let sealed = seal(&key, b"aad", b"secret cookie").unwrap();
        let opened = open(&key, b"aad", &sealed).unwrap();
        assert_eq!(&opened[..], b"secret cookie");
        assert!(open(&key, b"wrong", &sealed).is_err());
        let mut tampered = sealed.clone();
        let mut bytes = b64_decode(&tampered.ciphertext).unwrap();
        bytes[0] ^= 1;
        tampered.ciphertext = b64_encode(&bytes);
        assert!(open(&key, b"aad", &tampered).is_err());
        let other = random_key();
        assert!(open(&other, b"aad", &sealed).is_err());
    }

    #[test]
    fn nonces_are_unique_and_random() {
        let a = random_nonce();
        let b = random_nonce();
        assert_ne!(a, b);
        assert_eq!(a.len(), NONCE_LEN);
        assert_eq!(random_key().len(), KEY_LEN);
    }

    #[test]
    fn keyed_hash_depends_on_key_and_parts() {
        let k1 = random_key();
        let k2 = random_key();
        assert_eq!(keyed_hash(&k1, &[b"a", b"b"]), keyed_hash(&k1, &[b"a", b"b"]));
        assert_ne!(keyed_hash(&k1, &[b"a", b"b"]), keyed_hash(&k2, &[b"a", b"b"]));
        assert_ne!(keyed_hash(&k1, &[b"ab"]), keyed_hash(&k1, &[b"a", b"b"]));
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_plaintext_round_trips(data in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let key = random_key();
            let sealed = seal(&key, b"aad", &data).unwrap();
            let opened = open(&key, b"aad", &sealed).unwrap();
            assert_eq!(&opened[..], data.as_slice());
        }

        #[test]
        fn flipped_ciphertext_never_opens(bit in 0u8..8, idx in 0usize..32) {
            let key = random_key();
            let sealed = seal(&key, b"aad", &[7u8; 32]).unwrap();
            let mut bytes = b64_decode(&sealed.ciphertext).unwrap();
            if idx < bytes.len() {
                bytes[idx] ^= 1 << bit;
                let tampered = Sealed { nonce: sealed.nonce.clone(), ciphertext: b64_encode(&bytes) };
                assert!(open(&key, b"aad", &tampered).is_err());
            }
        }
    }
}
