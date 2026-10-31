use base64::Engine as _;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use subtle::ConstantTimeEq;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
    pub revoked: bool,
    pub user_agent: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PairRequest {
    pub pin: String,
    #[serde(default)]
    pub totp: Option<String>,
    #[serde(default)]
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairResponse {
    pub token: String,
    pub device: Device,
    pub warning: String,
}

#[derive(Debug, Default)]
struct Attempts {
    failures: u32,
    locked_until: Option<DateTime<Utc>>,
}

pub struct RemoteAuth {
    pin_hash: Option<[u8; 32]>,
    totp_secret: Option<Vec<u8>>,
    pub allowlist: Vec<IpAddr>,
    tokens: Mutex<HashMap<[u8; 32], Device>>,
    attempts: Mutex<HashMap<IpAddr, Attempts>>,
    pub max_failures: u32,
    pub lockout_seconds: i64,
}

impl std::fmt::Debug for RemoteAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteAuth")
            .field("pin", &self.pin_hash.is_some())
            .field("totp", &self.totp_secret.is_some())
            .field("devices", &self.tokens.lock().len())
            .finish()
    }
}

fn hash_pin(pin: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"rofarm-remote-pin-v1");
    hasher.update(pin.as_bytes());
    hasher.finalize().into()
}

fn hash_token(token: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"rofarm-device-token-v1");
    hasher.update(token.as_bytes());
    hasher.finalize().into()
}

impl RemoteAuth {
    pub fn new(pin: Option<&str>, totp_secret: Option<Vec<u8>>, allowlist: Vec<IpAddr>) -> Self {
        Self {
            pin_hash: pin.map(hash_pin),
            totp_secret,
            allowlist,
            tokens: Mutex::new(HashMap::new()),
            attempts: Mutex::new(HashMap::new()),
            max_failures: 5,
            lockout_seconds: 60,
        }
    }

    pub fn ip_allowed(&self, ip: IpAddr) -> bool {
        if ip.is_loopback() {
            return true;
        }
        self.allowlist.contains(&ip)
    }

    fn locked(&self, ip: IpAddr) -> Option<i64> {
        let mut attempts = self.attempts.lock();
        let entry = attempts.entry(ip).or_default();
        if let Some(until) = entry.locked_until {
            if until > Utc::now() {
                return Some((until - Utc::now()).num_seconds().max(1));
            }
            entry.locked_until = None;
            entry.failures = 0;
        }
        None
    }

    fn record_failure(&self, ip: IpAddr) {
        let mut attempts = self.attempts.lock();
        let entry = attempts.entry(ip).or_default();
        entry.failures += 1;
        if entry.failures >= self.max_failures {
            entry.locked_until = Some(Utc::now() + chrono::Duration::seconds(self.lockout_seconds));
            entry.failures = 0;
        }
    }

    pub fn pair(&self, ip: IpAddr, request: &PairRequest, user_agent: &str) -> Result<PairResponse, String> {
        if !self.ip_allowed(ip) {
            return Err("This address is not allowed.".into());
        }
        if let Some(seconds) = self.locked(ip) {
            return Err(format!("Too many attempts. Try again in {seconds}s."));
        }
        let Some(expected) = self.pin_hash else {
            return Err("Remote access is not configured.".into());
        };
        let actual = hash_pin(request.pin.trim());
        if expected.ct_eq(&actual).unwrap_u8() != 1 {
            self.record_failure(ip);
            return Err("Incorrect PIN.".into());
        }
        if let Some(secret) = &self.totp_secret {
            let code = request.totp.as_deref().unwrap_or("");
            if !verify_totp(secret, code, Utc::now()) {
                self.record_failure(ip);
                return Err("Incorrect or expired authenticator code.".into());
            }
        }
        let mut raw = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut raw);
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(raw);
        let device = Device {
            id: uuid::Uuid::new_v4().to_string(),
            name: if request.device_name.trim().is_empty() {
                "Paired device".into()
            } else {
                request.device_name.chars().take(64).collect()
            },
            created_at: Utc::now(),
            last_seen: Some(Utc::now()),
            revoked: false,
            user_agent: user_agent.chars().take(128).collect(),
        };
        self.tokens.lock().insert(hash_token(&token), device.clone());
        Ok(PairResponse {
            token,
            device,
            warning: "Store this token securely. It is shown once and never logged.".into(),
        })
    }

    pub fn authenticate(&self, token: &str) -> Option<Device> {
        let mut tokens = self.tokens.lock();
        let device = tokens.get_mut(&hash_token(token))?;
        if device.revoked {
            return None;
        }
        device.last_seen = Some(Utc::now());
        Some(device.clone())
    }

    pub fn revoke(&self, device_id: &str) -> bool {
        let mut tokens = self.tokens.lock();
        for device in tokens.values_mut() {
            if device.id == device_id {
                device.revoked = true;
                return true;
            }
        }
        false
    }

    pub fn devices(&self) -> Vec<Device> {
        self.tokens.lock().values().cloned().collect()
    }
}

pub fn verify_totp(secret: &[u8], code: &str, now: DateTime<Utc>) -> bool {
    let digits: Vec<u8> = code.chars().filter(|c| c.is_ascii_digit()).map(|c| c as u8 - b'0').collect();
    if digits.len() != 6 {
        return false;
    }
    let provided: u32 = digits.iter().fold(0u32, |acc, d| acc * 10 + *d as u32);
    let step = now.timestamp() / 30;
    (-1i64..=1).any(|offset| totp_value(secret, (step + offset) as u64) == provided)
}

pub fn totp_value(secret: &[u8], counter: u64) -> u32 {
    let mut msg = [0u8; 8];
    msg.copy_from_slice(&counter.to_be_bytes());
    let mut key = [0u8; 64];
    if secret.len() > 64 {
        let digest = Sha1::digest(secret);
        key[..20].copy_from_slice(&digest);
    } else {
        key[..secret.len()].copy_from_slice(secret);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }
    let mut inner = Sha1::new();
    inner.update(ipad);
    inner.update(msg);
    let inner_digest = inner.finalize();
    let mut outer = Sha1::new();
    outer.update(opad);
    outer.update(inner_digest);
    let digest = outer.finalize();
    let offset = (digest[19] & 0x0f) as usize;
    let value = ((digest[offset] as u32 & 0x7f) << 24)
        | ((digest[offset + 1] as u32) << 16)
        | ((digest[offset + 2] as u32) << 8)
        | digest[offset + 3] as u32;
    value % 1_000_000
}

use sha1::Sha1;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn pairing_needs_the_right_pin_and_is_rate_limited() {
        let auth = RemoteAuth::new(Some("482913"), None, vec![]);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let bad = auth.pair(ip, &PairRequest { pin: "000000".into(), totp: None, device_name: "x".into() }, "test");
        assert!(bad.is_err());
        for _ in 0..5 {
            let _ = auth.pair(ip, &PairRequest { pin: "000000".into(), totp: None, device_name: "x".into() }, "test");
        }
        assert!(auth.pair(ip, &PairRequest { pin: "482913".into(), totp: None, device_name: "x".into() }, "test").is_err());
    }

    #[test]
    fn issued_tokens_authenticate_and_revoke() {
        let auth = RemoteAuth::new(Some("123456"), None, vec![]);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let paired = auth
            .pair(ip, &PairRequest { pin: "123456".into(), totp: None, device_name: "Desk".into() }, "test-agent")
            .unwrap();
        let device = auth.authenticate(&paired.token).unwrap();
        assert_eq!(device.name, "Desk");
        assert!(auth.revoke(&device.id));
        assert!(auth.authenticate(&paired.token).is_none());
    }

    #[test]
    fn totp_matches_rfc_vectors() {
        let secret = b"12345678901234567890";
        let at = DateTime::from_timestamp(59, 0).unwrap();
        assert_eq!(totp_value(secret, 59 / 30), 287082);
        assert!(verify_totp(secret, "287082", at));
        assert!(!verify_totp(secret, "000000", at));
    }

    #[test]
    fn non_allowlisted_remote_ips_are_rejected() {
        let auth = RemoteAuth::new(Some("123456"), None, vec!["192.168.1.4".parse().unwrap()]);
        assert!(auth.ip_allowed("127.0.0.1".parse().unwrap()));
        assert!(auth.ip_allowed("192.168.1.4".parse().unwrap()));
        assert!(!auth.ip_allowed("8.8.8.8".parse().unwrap()));
    }
}
