use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("unsupported on this platform")]
    Unsupported,
    #[error("access denied")]
    AccessDenied,
    #[error("process not found")]
    NotFound,
    #[error("Windows API call failed: {0}")]
    Win32(&'static str),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("I/O error")]
    Io(#[from] std::io::Error),
}

impl PlatformError {
    pub fn code(&self) -> &'static str {
        match self {
            PlatformError::Unsupported => "unsupported",
            PlatformError::AccessDenied => "access_denied",
            PlatformError::NotFound => "not_found",
            PlatformError::Win32(_) => "win32",
            PlatformError::Invalid(_) => "invalid",
            PlatformError::Io(_) => "io",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformErrorPulse {
    pub code: &'static str,
    pub message: String,
}

impl From<&PlatformError> for PlatformErrorPulse {
    fn from(e: &PlatformError) -> Self {
        Self { code: e.code(), message: e.to_string() }
    }
}

pub type PlatformResult<T> = Result<T, PlatformError>;
