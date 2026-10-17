use std::{fmt, io};

#[derive(Debug)]
pub enum AppError {
    Configuration(String),
    Io {
        operation: &'static str,
        source: io::Error,
    },
    InvalidRequest(String),
    WorkerStopped,
    Backend(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => write!(f, "Configuration: {message}"),
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
            Self::InvalidRequest(message) => write!(f, "Invalid transaction: {message}"),
            Self::Backend(message) => write!(f, "Package data: {message}"),
            Self::WorkerStopped => write!(
                f,
                "Background task stopped unexpectedly. Retry the operation."
            ),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
