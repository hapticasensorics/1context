use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::ManagedPostgresError;

pub const MANIFEST_FILE_NAME: &str = "manifest.json";
pub const EXPECTED_POSTGRES_MAJOR: u16 = 17;
pub const REQUIRED_MANAGED_POSTGRES_EXTENSIONS: &[&str] = &[
    "timescaledb",
    "btree_gist",
    "pgcrypto",
    "pg_trgm",
    "vector",
    "pg_stat_statements",
];
pub const REQUIRED_MANAGED_POSTGRES_PRELOAD_LIBRARIES: &[&str] =
    &["timescaledb", "pg_stat_statements"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedPgManifestFile {
    pub bundle_schema: u16,
    pub arch: String,
    pub postgres_major: u16,
    pub postgres_version: String,
    pub timescale_version: String,
    pub build_id: String,
    pub bin: ManagedPgManifestBins,
    pub extension: ManagedPgManifestExtensions,
    #[serde(default)]
    pub required_extensions: Vec<String>,
    #[serde(default)]
    pub required_preload_libraries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedPgManifestBins {
    pub postgres: String,
    pub initdb: String,
    pub pg_ctl: String,
    pub pg_isready: String,
    pub psql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub createdb: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedPgManifestExtensions {
    pub timescaledb_control: String,
    pub timescaledb_library_glob: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPgManifest {
    pub prefix: PathBuf,
    pub manifest_path: PathBuf,
    pub bundle_schema: u16,
    pub arch: String,
    pub postgres_major: u16,
    pub postgres_version: String,
    pub timescale_version: String,
    pub build_id: String,
    pub postgres_bin: PathBuf,
    pub initdb_bin: PathBuf,
    pub pg_ctl_bin: PathBuf,
    pub pg_isready_bin: PathBuf,
    pub psql_bin: PathBuf,
    pub createdb_bin: Option<PathBuf>,
    pub timescaledb_control: PathBuf,
    pub timescaledb_library_glob: String,
    pub required_extensions: Vec<String>,
    pub required_preload_libraries: Vec<String>,
}

impl ManagedPgManifest {
    pub fn load_and_validate(prefix: impl Into<PathBuf>) -> Result<Self, ManagedPostgresError> {
        let mut prefix = prefix.into();
        if !prefix.exists() {
            return Err(ManagedPostgresError::BundleMissing { prefix });
        }
        prefix = std::fs::canonicalize(&prefix).unwrap_or(prefix);
        let manifest_path = prefix.join(MANIFEST_FILE_NAME);
        let bytes = std::fs::read(&manifest_path)
            .map_err(|source| ManagedPostgresError::io(&manifest_path, source))?;
        let manifest_file =
            serde_json::from_slice::<ManagedPgManifestFile>(&bytes).map_err(|error| {
                ManagedPostgresError::InvalidManifest {
                    path: manifest_path.clone(),
                    reason: error.to_string(),
                }
            })?;
        Self::from_file(prefix, manifest_path, manifest_file)?.validate()
    }

    pub fn from_file(
        prefix: PathBuf,
        manifest_path: PathBuf,
        manifest: ManagedPgManifestFile,
    ) -> Result<Self, ManagedPostgresError> {
        if manifest.bundle_schema != 1 {
            return Err(ManagedPostgresError::InvalidManifest {
                path: manifest_path,
                reason: format!("unsupported bundle_schema {}", manifest.bundle_schema),
            });
        }
        let required_extensions = if manifest.required_extensions.is_empty() {
            REQUIRED_MANAGED_POSTGRES_EXTENSIONS
                .iter()
                .map(ToString::to_string)
                .collect()
        } else {
            manifest.required_extensions
        };
        let required_preload_libraries = if manifest.required_preload_libraries.is_empty() {
            REQUIRED_MANAGED_POSTGRES_PRELOAD_LIBRARIES
                .iter()
                .map(ToString::to_string)
                .collect()
        } else {
            manifest.required_preload_libraries
        };

        Ok(Self {
            postgres_bin: prefix.join(&manifest.bin.postgres),
            initdb_bin: prefix.join(&manifest.bin.initdb),
            pg_ctl_bin: prefix.join(&manifest.bin.pg_ctl),
            pg_isready_bin: prefix.join(&manifest.bin.pg_isready),
            psql_bin: prefix.join(&manifest.bin.psql),
            createdb_bin: manifest.bin.createdb.as_ref().map(|path| prefix.join(path)),
            timescaledb_control: prefix.join(&manifest.extension.timescaledb_control),
            timescaledb_library_glob: manifest.extension.timescaledb_library_glob,
            bundle_schema: manifest.bundle_schema,
            arch: manifest.arch,
            postgres_major: manifest.postgres_major,
            postgres_version: manifest.postgres_version,
            timescale_version: manifest.timescale_version,
            build_id: manifest.build_id,
            required_extensions,
            required_preload_libraries,
            prefix,
            manifest_path,
        })
    }

    pub fn validate(self) -> Result<Self, ManagedPostgresError> {
        let actual_arch = current_manifest_arch();
        if self.arch != actual_arch {
            return Err(ManagedPostgresError::UnsupportedArch {
                expected: self.arch,
                actual: actual_arch,
            });
        }
        if self.postgres_major != EXPECTED_POSTGRES_MAJOR {
            return Err(ManagedPostgresError::bundle_invalid(
                &self.prefix,
                format!(
                    "postgres_major {} does not match expected {}",
                    self.postgres_major, EXPECTED_POSTGRES_MAJOR
                ),
            ));
        }
        for (label, path) in [
            ("postgres", &self.postgres_bin),
            ("initdb", &self.initdb_bin),
            ("pg_ctl", &self.pg_ctl_bin),
            ("pg_isready", &self.pg_isready_bin),
            ("psql", &self.psql_bin),
        ] {
            require_executable(&self.prefix, label, path)?;
        }
        if let Some(path) = &self.createdb_bin {
            require_executable(&self.prefix, "createdb", path)?;
        }
        if !self.timescaledb_control.is_file() {
            return Err(ManagedPostgresError::bundle_invalid(
                &self.prefix,
                format!(
                    "timescaledb control file is missing at {}",
                    self.timescaledb_control.display()
                ),
            ));
        }
        for extension in &self.required_extensions {
            let control_path = self.extension_dir().join(format!("{extension}.control"));
            require_regular_file(&self.prefix, extension, &control_path)?;
        }
        for preload_library in &self.required_preload_libraries {
            if !self
                .required_extensions
                .iter()
                .any(|extension| extension == preload_library)
            {
                return Err(ManagedPostgresError::bundle_invalid(
                    &self.prefix,
                    format!(
                        "preload library {preload_library} is not listed in required_extensions"
                    ),
                ));
            }
        }
        if !glob_has_match(&self.prefix, &self.timescaledb_library_glob)? {
            return Err(ManagedPostgresError::bundle_invalid(
                &self.prefix,
                format!(
                    "timescaledb library glob {:?} matched no files",
                    self.timescaledb_library_glob
                ),
            ));
        }
        Ok(self)
    }

    fn extension_dir(&self) -> PathBuf {
        self.timescaledb_control
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.prefix.join("share/postgresql/extension"))
    }
}

pub fn current_manifest_arch() -> String {
    match std::env::consts::ARCH {
        "aarch64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

fn require_executable(prefix: &Path, label: &str, path: &Path) -> Result<(), ManagedPostgresError> {
    let metadata = std::fs::metadata(path).map_err(|_| {
        ManagedPostgresError::bundle_invalid(
            prefix,
            format!("{label} binary is missing at {}", path.display()),
        )
    })?;
    if !metadata.is_file() {
        return Err(ManagedPostgresError::bundle_invalid(
            prefix,
            format!("{label} path is not a file: {}", path.display()),
        ));
    }
    if !is_executable(&metadata) {
        return Err(ManagedPostgresError::bundle_invalid(
            prefix,
            format!("{label} binary is not executable: {}", path.display()),
        ));
    }
    Ok(())
}

fn require_regular_file(
    prefix: &Path,
    label: &str,
    path: &Path,
) -> Result<(), ManagedPostgresError> {
    if !path.is_file() {
        return Err(ManagedPostgresError::bundle_invalid(
            prefix,
            format!("{label} file is missing at {}", path.display()),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    true
}

fn glob_has_match(prefix: &Path, pattern: &str) -> Result<bool, ManagedPostgresError> {
    let absolute_pattern = prefix.join(pattern);
    let Some(pattern_file_name) = absolute_pattern.file_name().and_then(|name| name.to_str())
    else {
        return Ok(false);
    };
    let parent = absolute_pattern.parent().unwrap_or(prefix);
    let (starts_with, ends_with) = pattern_file_name
        .split_once('*')
        .unwrap_or((pattern_file_name, ""));
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(ManagedPostgresError::io(parent, error)),
    };
    for entry in entries {
        let entry = entry.map_err(|source| ManagedPostgresError::io(parent, source))?;
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        if name.starts_with(starts_with) && name.ends_with(ends_with) {
            return Ok(true);
        }
    }
    Ok(false)
}
