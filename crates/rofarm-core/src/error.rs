use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("illegal transition: {0}")]
    Transition(#[from] crate::client::TransitionError),
    #[error("store error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("serialization error")]
    Serde(#[from] serde_json::Error),
    #[error("io error")]
    Io(#[from] std::io::Error),
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::NotFound(_) => "not_found",
            CoreError::Invalid(_) => "invalid",
            CoreError::Conflict(_) => "conflict",
            CoreError::Transition(_) => "illegal_transition",
            CoreError::Store(_) => "store",
            CoreError::Serde(_) => "serde",
            CoreError::Io(_) => "io",
        }
    }

    pub fn detail(&self) -> String {
        self.to_string()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorPulse {
    pub code: &'static str,
    pub message: String,
}

impl From<&CoreError> for ErrorPulse {
    fn from(e: &CoreError) -> Self {
        Self { code: e.code(), message: e.to_string() }
    }
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
