use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

use crate::crypto::{random_bytes, SALT_LEN};
use crate::error::{VaultError, VaultResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub algorithm: String,
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    #[serde(with = "crate::crypto::b64")]
    pub salt: Vec<u8>,
}

impl KdfParams {
    pub fn new_random(m_cost_kib: u32, t_cost: u32, p_cost: u32) -> Self {
        Self {
            algorithm: "argon2id".into(),
            m_cost_kib,
            t_cost,
            p_cost,
            salt: random_bytes::<SALT_LEN>().to_vec(),
        }
    }

    pub fn validate(&self) -> VaultResult<()> {
        if self.algorithm != "argon2id" {
            return Err(VaultError::KdfLimits);
        }
        if !(8 * 1024..=1024 * 1024).contains(&self.m_cost_kib) {
            return Err(VaultError::KdfLimits);
        }
        if !(1..=16).contains(&self.t_cost) || !(1..=16).contains(&self.p_cost) {
            return Err(VaultError::KdfLimits);
        }
        if !(16..=64).contains(&self.salt.len()) {
            return Err(VaultError::KdfLimits);
        }
        Ok(())
    }

    pub fn testing() -> Self {
        Self { algorithm: "argon2id".into(), m_cost_kib: 8 * 1024, t_cost: 1, p_cost: 1, salt: random_bytes::<SALT_LEN>().to_vec() }
    }

    pub fn params(&self) -> VaultResult<Params> {
        Params::new(self.m_cost_kib, self.t_cost, self.p_cost, None).map_err(|_| VaultError::KdfLimits)
    }
}

pub fn derive(passphrase: &str, params: &KdfParams) -> VaultResult<Zeroizing<[u8; 32]>> {
    params.validate()?;
    if passphrase.is_empty() {
        return Err(VaultError::Invalid("Passphrase must not be empty.".into()));
    }
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.params()?);
    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(passphrase.as_bytes(), &params.salt, out.as_mut())
        .map_err(|_| VaultError::Crypto)?;
    Ok(out)
}

pub fn measure(params: &KdfParams) -> Duration {
    let start = Instant::now();
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.params().expect("valid"));
    let mut out = Zeroizing::new([0u8; 32]);
    let _ = argon.hash_password_into(b"calibration", &params.salt, out.as_mut());
    start.elapsed()
}

pub fn calibrate(target_ms: u64, allow_low_memory: bool) -> KdfParams {
    let cores = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4).clamp(1, 4);
    let min_mib = if allow_low_memory { 8 } else { 64 };
    let mut best = KdfParams::new_random(64 * 1024, 3, cores);
    let mut best_delta = f64::INFINITY;
    for _ in 0..3 {
        let elapsed = measure(&best).as_secs_f64() * 1000.0;
        let delta = (elapsed - target_ms as f64).abs();
        if delta < best_delta {
            best_delta = delta;
        }
        if target_ms > 0 && elapsed > 0.0 {
            let scale = target_ms as f64 / elapsed;
            let new_mib = ((best.m_cost_kib as f64 * scale) / 1024.0).round() as u32;
            let new_mib = new_mib.clamp(min_mib, 1024);
            let new_t = if new_mib <= best.m_cost_kib { (best.t_cost as f64 * scale).round().clamp(2.0, 8.0) as u32 } else { best.t_cost };
            if new_mib == best.m_cost_kib && new_t == best.t_cost {
                break;
            }
            best.m_cost_kib = new_mib * 1024;
            best.t_cost = new_t.max(2);
            best.p_cost = cores;
            best.salt = random_bytes::<SALT_LEN>().to_vec();
        }
        if best_delta < target_ms as f64 * 0.2 {
            break;
        }
    }
    best
}

const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const RECOVERY_LEN: usize = 9;

fn crockford_value(c: char) -> Option<u8> {
    let upper = c.to_ascii_uppercase();
    let mapped = match upper {
        'I' | 'L' => '1',
        'O' => '0',
        _ => upper,
    } as u8;
    CROCKFORD.iter().position(|&x| x == mapped).map(|p| p as u8)
}

fn recovery_checksum(chars: &[u8]) -> u8 {
    let mut sum: u32 = 0;
    for (idx, c) in chars.iter().enumerate() {
        sum += (*c as u32) * (idx as u32 + 3);
    }
    (sum % 32) as u8
}

pub fn generate_recovery_code() -> String {
    let mut rng = rand::thread_rng();
    let mut raw = [0u8; 8];
    rng.fill_bytes(&mut raw);
    let mut chars: Vec<u8> = raw.iter().map(|b| b & 0x1f).collect();
    let checksum = recovery_checksum(&chars);
    chars.push(checksum);
    chars.iter().map(|v| CROCKFORD[*v as usize] as char).collect()
}

pub fn normalize_recovery_code(input: &str) -> VaultResult<String> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    if cleaned.len() != RECOVERY_LEN {
        return Err(VaultError::Invalid("Recovery code must have 9 characters.".into()));
    }
    let mut values = Vec::with_capacity(RECOVERY_LEN);
    for c in cleaned.chars() {
        values.push(crockford_value(c).ok_or_else(|| VaultError::Invalid("Recovery code has an invalid character.".into()))?);
    }
    let checksum = values.pop().unwrap();
    if recovery_checksum(&values) != checksum {
        return Err(VaultError::Invalid("Recovery code checksum does not match.".into()));
    }
    Ok(values.iter().map(|v| CROCKFORD[*v as usize] as char).chain(std::iter::once(CROCKFORD[checksum as usize] as char)).collect())
}

pub fn recovery_code_is_valid(input: &str) -> bool {
    normalize_recovery_code(input).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    pub x: u8,
    #[serde(with = "crate::crypto::b64")]
    pub y: Vec<u8>,
}

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    for _ in 0..8 {
        if b & 1 != 0 {
            p ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    p
}

fn gf_pow(mut a: u8, mut e: u16) -> u8 {
    let mut out = 1u8;
    while e > 0 {
        if e & 1 == 1 {
            out = gf_mul(out, a);
        }
        a = gf_mul(a, a);
        e >>= 1;
    }
    out
}

fn gf_inv(a: u8) -> u8 {
    gf_pow(a, 254)
}

pub fn split_secret(secret: &[u8], threshold: u8, shares: u8) -> VaultResult<Vec<Share>> {
    if !(2..=8).contains(&threshold) || shares < threshold || shares > 16 || secret.is_empty() || secret.len() > 128 {
        return Err(VaultError::Invalid("Shamir needs 2-8 threshold, 2-16 shares, 1-128 byte secret.".into()));
    }
    let mut rng = rand::thread_rng();
    let mut out: Vec<Share> = (1..=shares).map(|x| Share { x, y: vec![0u8; secret.len()] }).collect();
    for (byte_idx, secret_byte) in secret.iter().enumerate() {
        let mut coeffs = vec![*secret_byte];
        let mut buf = vec![0u8; threshold as usize - 1];
        rng.fill_bytes(&mut buf);
        coeffs.extend_from_slice(&buf);
        for share in out.iter_mut() {
            let mut value = 0u8;
            for (power, coeff) in coeffs.iter().enumerate() {
                value ^= gf_mul(*coeff, gf_pow(share.x, power as u16));
            }
            share.y[byte_idx] = value;
        }
    }
    Ok(out)
}

pub fn combine_shares(shares: &[Share]) -> VaultResult<Zeroizing<Vec<u8>>> {
    if shares.len() < 2 {
        return Err(VaultError::Invalid("At least two shares are required.".into()));
    }
    let len = shares[0].y.len();
    if shares.iter().any(|s| s.y.len() != len || s.x == 0) {
        return Err(VaultError::Invalid("Shares are malformed.".into()));
    }
    let mut seen_x = std::collections::BTreeSet::new();
    if shares.iter().any(|s| !seen_x.insert(s.x)) {
        return Err(VaultError::Invalid("Duplicate share indices.".into()));
    }
    let mut secret = vec![0u8; len];
    for byte_idx in 0..len {
        let mut acc = 0u8;
        for (i, si) in shares.iter().enumerate() {
            let mut num = 1u8;
            let mut den = 1u8;
            for (j, sj) in shares.iter().enumerate() {
                if i == j {
                    continue;
                }
                num = gf_mul(num, sj.x);
                den = gf_mul(den, si.x ^ sj.x);
            }
            if den == 0 {
                return Err(VaultError::Invalid("Invalid share set.".into()));
            }
            let basis = gf_mul(num, gf_inv(den));
            acc ^= gf_mul(si.y[byte_idx], basis);
        }
        secret[byte_idx] = acc;
    }
    Ok(Zeroizing::new(secret))
}

pub fn encode_share(share: &Share) -> String {
    let mut check_input = vec![share.x];
    check_input.push(share.y.len() as u8);
    format!("ROFSHARE1-{}-{}-{}", share.x, crate::crypto::b64_encode(&share.y), &blake3::hash(&check_input).to_hex()[..8])
}

pub fn decode_share(text: &str) -> VaultResult<Share> {
    let rest = text.trim().strip_prefix("ROFSHARE1-").ok_or_else(|| VaultError::Invalid("Not a RoFarm share.".into()))?;
    let mut parts = rest.split('-');
    let x: u8 = parts.next().ok_or_else(|| VaultError::Invalid("Share is incomplete.".into()))?.parse().map_err(|_| VaultError::Invalid("Bad share index.".into()))?;
    let y = crate::crypto::b64_decode(parts.next().ok_or_else(|| VaultError::Invalid("Share is incomplete.".into()))?).map_err(|_| VaultError::Invalid("Bad share data.".into()))?;
    let checksum = parts.next().ok_or_else(|| VaultError::Invalid("Share is incomplete.".into()))?;
    let mut check_input = vec![x];
    check_input.push(y.len() as u8);
    let expected = &blake3::hash(&check_input).to_hex()[..8];
    if checksum != expected {
        return Err(VaultError::Invalid("Share checksum mismatch.".into()));
    }
    Ok(Share { x, y })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn params_validate_and_testing_params_are_cheap() {
        let p = KdfParams::testing();
        p.validate().unwrap();
        assert!(p.m_cost_kib < 64 * 1024);
        let mut bad = p.clone();
        bad.m_cost_kib = 2;
        assert!(bad.validate().is_err());
        bad = p.clone();
        bad.salt = vec![1, 2, 3];
        assert!(bad.validate().is_err());
    }

    #[test]
    fn derive_is_deterministic_for_same_salt_and_differs_otherwise() {
        let p = KdfParams::testing();
        let a = derive("correct horse", &p).unwrap();
        let b = derive("correct horse", &p).unwrap();
        let c = derive("wrong", &p).unwrap();
        assert_eq!(&a[..], &b[..]);
        assert_ne!(&a[..], &c[..]);
        let mut p2 = p.clone();
        p2.salt = random_bytes::<SALT_LEN>().to_vec();
        let d = derive("correct horse", &p2).unwrap();
        assert_ne!(&a[..], &d[..]);
    }

    #[test]
    fn recovery_codes_are_nine_chars_and_validate_checksum() {
        for _ in 0..50 {
            let code = generate_recovery_code();
            assert_eq!(code.len(), 9);
            assert!(recovery_code_is_valid(&code), "{code}");
            let normalized = normalize_recovery_code(&code).unwrap();
            assert_eq!(normalized, code);
            let mangled = code.replace('0', "O").replace('1', "I");
            assert_eq!(normalize_recovery_code(&mangled).unwrap(), code);
        }
        assert!(!recovery_code_is_valid("AAAAAAAAA"));
        assert!(!recovery_code_is_valid("short"));
        assert!(!recovery_code_is_valid("!!!!!!!!!"));
    }

    #[test]
    fn shamir_reconstructs_from_any_threshold_subset() {
        let secret = random_bytes::<32>();
        let shares = split_secret(&secret, 2, 5).unwrap();
        assert_eq!(shares.len(), 5);
        for i in 0..shares.len() {
            for j in i + 1..shares.len() {
                let combined = combine_shares(&[shares[i].clone(), shares[j].clone()]).unwrap();
                assert_eq!(&combined[..], &secret[..]);
            }
        }
        for s in &shares {
            let encoded = encode_share(s);
            let decoded = decode_share(&encoded).unwrap();
            assert_eq!(decoded, *s);
        }
    }

    #[test]
    fn shamir_three_of_five_and_bad_inputs() {
        let secret = b"master-key-material-32-bytes!!!!!";
        let shares = split_secret(secret, 3, 5).unwrap();
        let combined = combine_shares(&[shares[0].clone(), shares[2].clone(), shares[4].clone()]).unwrap();
        assert_eq!(&combined[..], secret);
        assert!(combine_shares(&[shares[0].clone()]).is_err());
        assert!(combine_shares(&[shares[0].clone(), shares[0].clone()]).is_err());
        assert!(split_secret(secret, 2, 1).is_err());
        assert!(decode_share("ROFSHARE1-9-AAAA-deadbeef").is_err());
    }

    #[test]
    fn calibration_stays_within_bounds() {
        let params = calibrate(40, true);
        params.validate().unwrap();
        assert!(params.m_cost_kib >= 8 * 1024);
        assert!(params.t_cost >= 2);
    }

    proptest::proptest! {
        #[test]
        fn any_subset_of_two_reconstructs(secret in proptest::collection::vec(any::<u8>(), 1..64)) {
            let shares = split_secret(&secret, 2, 4).unwrap();
            let combined = combine_shares(&[shares[1].clone(), shares[3].clone()]).unwrap();
            prop_assert_eq!(&combined[..], secret.as_slice());
        }
    }
}
