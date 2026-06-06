use std::path::PathBuf;

#[derive(Debug)]
pub enum ManagedPostgresError {
    BootstrapLocked {
        path: PathBuf,
    },
    BundleMissing {
        prefix: PathBuf,
    },
    BundleInvalid {
        prefix: PathBuf,
        reason: String,
    },
    InvalidManifest {
        path: PathBuf,
        reason: String,
    },
    UnsupportedArch {
        expected: String,
        actual: String,
    },
    CommandFailed {
        program: PathBuf,
        args: Vec<String>,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
    SchemaBootstrap {
        source: crate::schema::CurrentSchemaError,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl ManagedPostgresError {
    pub fn bundle_invalid(prefix: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::BundleInvalid {
            prefix: prefix.into(),
            reason: reason.into(),
        }
    }

    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn command_failed(
        program: impl Into<PathBuf>,
        args: Vec<String>,
        status: Option<i32>,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
    ) -> Self {
        Self::CommandFailed {
            program: program.into(),
            args,
            status,
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }
}

impl std::fmt::Display for ManagedPostgresError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BootstrapLocked { path } => {
                write!(
                    formatter,
                    "managed Postgres bootstrap is already running; lock exists at {}",
                    path.display()
                )
            }
            Self::BundleMissing { prefix } => {
                write!(
                    formatter,
                    "managed Postgres bundle is missing at {}",
                    prefix.display()
                )
            }
            Self::BundleInvalid { prefix, reason } => {
                write!(
                    formatter,
                    "managed Postgres bundle at {} is invalid: {reason}",
                    prefix.display()
                )
            }
            Self::InvalidManifest { path, reason } => {
                write!(
                    formatter,
                    "managed Postgres manifest {} is invalid: {reason}",
                    path.display()
                )
            }
            Self::UnsupportedArch { expected, actual } => {
                write!(
                    formatter,
                    "managed Postgres bundle arch {expected:?} does not match current arch {actual:?}"
                )
            }
            Self::CommandFailed {
                program,
                args,
                status,
                stdout,
                stderr,
            } => {
                write!(
                    formatter,
                    "managed Postgres command failed: {} {:?} exited with status {:?}",
                    program.display(),
                    args,
                    status
                )?;
                if !stderr.trim().is_empty() {
                    write!(formatter, "; stderr={:?}", stderr.trim())?;
                }
                if !stdout.trim().is_empty() {
                    write!(formatter, "; stdout={:?}", stdout.trim())?;
                }
                Ok(())
            }
            Self::SchemaBootstrap { source } => {
                write!(
                    formatter,
                    "managed Postgres schema bootstrap failed: {source}"
                )
            }
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for ManagedPostgresError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SchemaBootstrap { source } => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
