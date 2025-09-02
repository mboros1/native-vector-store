use thiserror::Error;

#[derive(Debug, Error)]
pub enum NvsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid manifest: {0}")]
    InvalidManifest(&'static str),

    #[error("invalid bundle: {0}")]
    InvalidBundle(&'static str),
}

pub type Result<T> = std::result::Result<T, NvsError>;
