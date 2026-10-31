use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum RobloxError {
    #[error("network request failed")]
    Transport,
    #[error("Roblox rejected the request (HTTP {status})")]
    Http { status: u16 },
    #[error("Roblox returned an unexpected payload")]
    Decode,
    #[error("session is not valid")]
    Unauthorized,
    #[error("Roblox requires 2-step verification")]
    TwoStepRequired,
    #[error("rate limited by Roblox")]
    RateLimited { retry_after_secs: u64 },
    #[error("host blocked by the network allow-list: {0}")]
    Blocked(String),
    #[error("not found")]
    NotFound,
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("local I/O failed")]
    Io(#[from] std::io::Error),
}

impl RobloxError {
    pub fn code(&self) -> &'static str {
        match self {
            RobloxError::Transport => "transport",
            RobloxError::Http { .. } => "http",
            RobloxError::Decode => "decode",
            RobloxError::Unauthorized => "unauthorized",
            RobloxError::TwoStepRequired => "two_step",
            RobloxError::RateLimited { .. } => "rate_limited",
            RobloxError::Blocked(_) => "blocked_host",
            RobloxError::NotFound => "not_found",
            RobloxError::Invalid(_) => "invalid",
            RobloxError::Io(_) => "io",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RobloxErrorPulse {
    pub code: &'static str,
    pub message: String,
}

impl From<&RobloxError> for RobloxErrorPulse {
    fn from(e: &RobloxError) -> Self {
        Self { code: e.code(), message: e.to_string() }
    }
}

pub type RobloxResult<T> = Result<T, RobloxError>;
