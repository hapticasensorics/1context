use std::error::Error;
use std::fmt;
use std::path::PathBuf;

pub type CaptureCoreResult<T> = Result<T, CaptureCoreError>;

#[derive(Debug)]
pub enum CaptureCoreError {
    Io {
        path: Option<PathBuf>,
        source: std::io::Error,
    },
    Json {
        path: Option<PathBuf>,
        source: serde_json::Error,
    },
    InvalidPath {
        path: String,
        message: String,
    },
    InvalidState(String),
    InvalidTimeRange(String),
}

impl CaptureCoreError {
    pub fn io(path: impl Into<Option<PathBuf>>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn json(path: impl Into<Option<PathBuf>>, source: serde_json::Error) -> Self {
        Self::Json {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for CaptureCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => match path {
                Some(path) => write!(formatter, "I/O error at {}: {source}", path.display()),
                None => write!(formatter, "I/O error: {source}"),
            },
            Self::Json { path, source } => match path {
                Some(path) => write!(formatter, "JSON error at {}: {source}", path.display()),
                None => write!(formatter, "JSON error: {source}"),
            },
            Self::InvalidPath { path, message } => {
                write!(formatter, "invalid path {path:?}: {message}")
            }
            Self::InvalidState(message) => formatter.write_str(message),
            Self::InvalidTimeRange(message) => formatter.write_str(message),
        }
    }
}

impl Error for CaptureCoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CaptureCoreError {
    fn from(source: std::io::Error) -> Self {
        Self::io(None, source)
    }
}

impl From<serde_json::Error> for CaptureCoreError {
    fn from(source: serde_json::Error) -> Self {
        Self::json(None, source)
    }
}
