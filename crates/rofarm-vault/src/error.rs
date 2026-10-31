use serde::Serialize;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("vault is locked")]
    Locked,
    #[error("authentication failed")]
    AuthenticationFailed,
    #[error("vault data is corrupt")]
    Corrupt,
    #[error("unsupported vault version {0}")]
    UnsupportedVersion(u16),
    #[error("record not found")]
    NotFound,
    #[error("record already exists")]
    AlreadyExists,
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("too many unlock attempts; retry in {0:?}")]
    TooManyAttempts(Duration),
    #[error("security binding is unavailable on this machine")]
    BindingUnavailable,
    #[error("this machine cannot unseal the binding")]
    BindingFailed,
    #[error("KDF parameters are outside safe limits")]
    KdfLimits,
    #[error("backup format is invalid: {0}")]
    BackupInvalid(String),
    #[error("backup passphrase is wrong or the file is corrupt")]
    BackupAuthenticationFailed,
    #[error("path rejected: {0}")]
    UnsafePath(String),
    #[error("I/O error")]
    Io(#[from] std::io::Error),
    #[error("serialization error")]
    Serde(#[from] serde_json::Error),
    #[error("cryptographic failure")]
    Crypto,
}

impl VaultError {
    pub fn code(&self) -> &'static str {
        match self {
            VaultError::Locked => "vault_locked",
            VaultError::AuthenticationFailed => "vault_auth_failed",
            VaultError::Corrupt => "vault_corrupt",
            VaultError::UnsupportedVersion(_) => "vault_version",
            VaultError::NotFound => "not_found",
            VaultError::AlreadyExists => "already_exists",
            VaultError::Invalid(_) => "invalid",
            VaultError::TooManyAttempts(_) => "too_many_attempts",
            VaultError::BindingUnavailable => "binding_unavailable",
            VaultError::BindingFailed => "binding_failed",
            VaultError::KdfLimits => "kdf_limits",
            VaultError::BackupInvalid(_) => "backup_invalid",
            VaultError::BackupAuthenticationFailed => "backup_auth_failed",
            VaultError::UnsafePath(_) => "unsafe_path",
            VaultError::Io(_) => "io",
            VaultError::Serde(_) => "serde",
            VaultError::Crypto => "crypto",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VaultErrorPulse {
    pub code: &'static str,
    pub message: String,
    pub retry_after_ms: Option<u64>,
}

impl From<&VaultError> for VaultErrorPulse {
    fn from(e: &VaultError) -> Self {
        Self {
            code: e.code(),
            message: e.to_string(),
            retry_after_ms: match e {
                VaultError::TooManyAttempts(d) => Some(d.as_millis() as u64),
                _ => None,
            },
        }
    }
}

pub type VaultResult<T> = Result<T, VaultError>;
