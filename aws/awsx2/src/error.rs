use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("AWS CLI error: {0}")]
    AwsCli(String),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No instance found matching: {0}")]
    NoInstance(String),
    #[error("Multiple instances found matching: {0}")]
    MultipleInstances(String),
    #[error("Tunnel error: {0}")]
    Tunnel(String),
    #[error("No SSM-online bastions found")]
    NoBastions,
    #[error("Port {0} is not open after timeout")]
    PortClosed(u16),
    #[allow(dead_code)]
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
