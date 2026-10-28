use crate::auth::{Grant, Role};
use crate::error::{Result, ServiceError};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    path::PathBuf,
};

/// Operator-owned configuration. Credentials are mounted separately and never accepted as tools.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub listen: SocketAddr,
    pub public_url: String,
    pub issuer: String,
    pub jwks_file: PathBuf,
    pub database: PathBuf,
    pub journal: PathBuf,
    #[serde(default)]
    pub storage_encryption: Option<StorageEncryption>,
    pub grants: BTreeMap<String, Grant>,
    #[serde(default)]
    pub owner_authentication: OwnerAuthentication,
    #[serde(default)]
    pub allowed_origins: BTreeSet<String>,
    #[serde(default)]
    pub revoked_token_ids: BTreeSet<String>,
    #[serde(default)]
    pub allow_live: bool,
    #[serde(default = "token_lifetime")]
    pub max_token_lifetime_seconds: u64,
    #[serde(default = "reconcile_interval")]
    pub reconcile_interval_seconds: u64,
    #[serde(default = "queue_capacity")]
    pub queue_capacity: usize,
    #[serde(default = "request_timeout")]
    pub request_timeout_ms: u64,
    #[serde(default = "queue_timeout")]
    pub queue_timeout_ms: u64,
    #[serde(default)]
    pub distribution_password_file: Option<PathBuf>,
    #[serde(default)]
    pub calendar_credentials_file: Option<PathBuf>,
}

/// Independent raw keys provisioned outside the data volume; never tool inputs.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageEncryption {
    pub financial_key_file: PathBuf,
    pub journal_key_file: PathBuf,
    pub backup_key_file: PathBuf,
}

/// The operator must map these ACR values to enforced human MFA at the issuer.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerAuthentication {
    pub acr_values: BTreeSet<String>,
    pub max_age_seconds: u64,
}

impl Default for OwnerAuthentication {
    fn default() -> Self {
        Self {
            acr_values: ["urn:trust:owner:mfa".to_owned()].into_iter().collect(),
            max_age_seconds: 300,
        }
    }
}

fn token_lifetime() -> u64 {
    3600
}
fn reconcile_interval() -> u64 {
    30
}
fn queue_capacity() -> usize {
    64
}
fn request_timeout() -> u64 {
    45000
}
fn queue_timeout() -> u64 {
    40000
}

impl Config {
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let bytes = trust_security::read_public_config(path, 1024 * 1024)
            .map_err(|_| ServiceError::invalid("Cannot read private operator configuration; reject symlinks and group/other-writable files or directories."))?;
        let config: Self = serde_json::from_slice(&bytes).map_err(|_| {
            ServiceError::invalid(
                "Invalid service configuration; check required fields and unknown keys.",
            )
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        let public = url::Url::parse(&self.public_url)
            .map_err(|_| ServiceError::invalid("public_url must be an absolute URL."))?;
        let issuer = url::Url::parse(&self.issuer)
            .map_err(|_| ServiceError::invalid("issuer must be an absolute URL."))?;
        for url in [&public, &issuer] {
            if url.username() != ""
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(ServiceError::invalid(
                    "Public URLs must not contain credentials, query parameters or fragments.",
                ));
            }
            let loopback = url.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            if url.scheme() != "https"
                && !(url.scheme() == "http" && loopback && self.listen.ip().is_loopback())
            {
                return Err(ServiceError::invalid(
                    "HTTPS is required; plain HTTP is permitted only for loopback development.",
                ));
            }
        }
        if public.path() != "/" || self.public_url.ends_with('/') {
            return Err(ServiceError::invalid(
                "public_url must be an origin without a trailing slash or path.",
            ));
        }
        let development =
            self.listen.ip().is_loopback() && public.scheme() == "http" && !self.allow_live;
        if !development && self.storage_encryption.is_none() {
            return Err(ServiceError::invalid("SQLCipher storage_encryption is required outside loopback HTTP development and for all live trading."));
        }
        if let Some(encryption) = &self.storage_encryption {
            let paths = [
                &encryption.financial_key_file,
                &encryption.journal_key_file,
                &encryption.backup_key_file,
            ];
            let unique: BTreeSet<_> = paths.iter().collect();
            if unique.len() != paths.len()
                || paths.iter().any(|path| {
                    !path.is_absolute()
                        || path
                            .components()
                            .any(|part| matches!(part, std::path::Component::ParentDir))
                        || [&self.database, &self.journal].iter().any(|data| {
                            data.parent().is_some_and(|parent| path.starts_with(parent))
                        })
                })
                || !self.database.is_absolute()
                || !self.journal.is_absolute()
                || [&self.database, &self.journal].iter().any(|path| {
                    path.components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                })
            {
                return Err(ServiceError::invalid("Use absolute database paths and three distinct key files outside both data directories; parent traversal is forbidden."));
            }
        }
        if self.database == self.journal
            || self.queue_capacity == 0
            || self.queue_capacity > 1024
            || self.max_token_lifetime_seconds == 0
            || self.max_token_lifetime_seconds > 86400
        {
            return Err(ServiceError::invalid("Use separate database/journal files, queue capacity 1..1024, and token lifetime 1..86400 seconds."));
        }
        if self.grants.is_empty()
            || self
                .grants
                .keys()
                .any(|sub| sub.is_empty() || sub.len() > 256)
        {
            return Err(ServiceError::invalid(
                "Configure at least one explicit subject grant.",
            ));
        }
        if self.request_timeout_ms == 0
            || self.request_timeout_ms > 45000
            || self.queue_timeout_ms == 0
            || self.queue_timeout_ms > self.request_timeout_ms
        {
            return Err(ServiceError::invalid(
                "Use request_timeout_ms 1..45000 and queue_timeout_ms 1..request_timeout_ms.",
            ));
        }
        for grant in self.grants.values() {
            grant.validate()?;
        }
        if self
            .grants
            .values()
            .filter(|grant| grant.role == Role::Owner)
            .count()
            != 1
        {
            return Err(ServiceError::invalid(
                "Configure exactly one explicit owner subject; all other identities are agents.",
            ));
        }
        let owner = &self.owner_authentication;
        if owner.max_age_seconds == 0
            || owner.max_age_seconds > 900
            || owner.acr_values.is_empty()
            || owner.acr_values.len() > 8
            || owner.acr_values.iter().any(|acr| {
                acr.is_empty()
                    || acr.len() > 256
                    || !acr
                        .bytes()
                        .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\\'))
            })
        {
            return Err(ServiceError::invalid("Owner authentication requires 1..8 exact MFA ACR values without spaces, quotes or backslashes, and max_age_seconds 1..900."));
        }
        Ok(())
    }

    pub fn resource(&self) -> String {
        format!("{}/mcp", self.public_url)
    }

    pub fn open_financial_database(&self) -> Result<db_sqlite::SqliteDatabase> {
        if let Some(encryption) = &self.storage_encryption {
            let key = trust_security::DatabaseKey::load(&encryption.financial_key_file)
                .map_err(|_| storage_error())?;
            db_sqlite::SqliteDatabase::open_encrypted(&self.database, &key)
                .map_err(|_| storage_error())
        } else {
            Ok(db_sqlite::SqliteDatabase::new(
                self.database.to_str().ok_or_else(storage_error)?,
            ))
        }
    }

    pub(crate) fn verify_secret_files(&self) -> Result<()> {
        if let Some(encryption) = &self.storage_encryption {
            let paths = [
                &encryption.financial_key_file,
                &encryption.journal_key_file,
                &encryption.backup_key_file,
            ];
            let keys = paths
                .iter()
                .map(|path| trust_security::DatabaseKey::load(path).map_err(|_| storage_error()))
                .collect::<Result<Vec<_>>>()?;
            for (index, key) in keys.iter().enumerate() {
                if keys
                    .iter()
                    .skip(index.saturating_add(1))
                    .any(|other| key.same_as(other))
                {
                    return Err(ServiceError::invalid(
                        "Financial, journal and backup keys must be independent.",
                    ));
                }
            }
            // HTTPS deployments and live operation require RAM-backed secret mounts.
            if !self.listen.ip().is_loopback()
                || self.public_url.starts_with("https:")
                || self.allow_live
            {
                let broker_secret =
                    std::env::var_os("TRUST_ALPACA_CREDENTIALS_FILE").map(PathBuf::from);
                for path in paths
                    .into_iter()
                    .chain(self.distribution_password_file.iter())
                    .chain(self.calendar_credentials_file.iter())
                    .chain(broker_secret.iter())
                {
                    trust_security::require_memory_file(path).map_err(|_| ServiceError::invalid("Production secrets must be private regular files on tmpfs/ramfs. Provision them from encrypted secret storage; disk-backed or symlinked mounts are refused."))?;
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn storage_error() -> ServiceError {
    ServiceError::new("storage_unavailable", "Protected storage could not be opened or verified.", "Check private file permissions, mounted keys and SQLCipher format. Migrate existing plaintext storage offline; never replace a missing key with a new one.")
}

#[cfg(all(test, target_os = "linux"))]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use trust_security::DatabaseKey;

    fn fixture(root: &std::path::Path) -> Config {
        serde_json::from_value(serde_json::json!({
            "listen":"127.0.0.1:8000", "public_url":"http://localhost:8000",
            "issuer":"http://localhost:8080", "jwks_file": root.join("jwks.json"),
            "database": root.join("data/financial.db"), "journal":root.join("data/control.db"),
            "grants":{"owner":{"role":"owner","all_accounts":true,"scopes":["trust:admin"]}}
        }))
        .unwrap()
    }
    fn encryption(root: &std::path::Path) -> StorageEncryption {
        StorageEncryption {
            financial_key_file: root.join("secrets/financial-key.hex"),
            journal_key_file: root.join("secrets/journal-key.hex"),
            backup_key_file: root.join("secrets/backup-key.hex"),
        }
    }
    #[test]
    fn production_requires_independent_ram_keys_and_private_files() {
        let dir = tempfile::tempdir_in("/dev/shm").unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut config = fixture(dir.path());
        config.validate().unwrap();
        config.public_url = "https://trust.test".into();
        assert!(config.validate().is_err());
        config.storage_encryption = Some(encryption(dir.path()));
        config.validate().unwrap();
        assert!(config.verify_secret_files().is_err());
        let keys = config.storage_encryption.as_ref().unwrap();
        for path in [
            &keys.financial_key_file,
            &keys.journal_key_file,
            &keys.backup_key_file,
        ] {
            DatabaseKey::generate().unwrap().write_new(path).unwrap();
        }
        config.verify_secret_files().unwrap();
        std::fs::set_permissions(
            &keys.financial_key_file,
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(config.verify_secret_files().is_err());
        std::fs::set_permissions(
            &keys.financial_key_file,
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        std::fs::copy(&keys.financial_key_file, &keys.journal_key_file).unwrap();
        assert!(config.verify_secret_files().is_err());
        let mut missing = fixture(dir.path());
        missing.allow_live = true;
        assert!(missing.validate().is_err());
    }
    #[test]
    fn encrypted_configuration_rejects_data_colocated_keys_and_path_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = fixture(dir.path());
        let keys = encryption(dir.path());
        config.storage_encryption = Some(keys.clone());
        config.validate().unwrap();
        config.storage_encryption.as_mut().unwrap().backup_key_file =
            config.database.with_file_name("backup-key.hex");
        assert!(config.validate().is_err());
        config.storage_encryption = Some(keys.clone());
        config.storage_encryption.as_mut().unwrap().backup_key_file = keys.financial_key_file;
        assert!(config.validate().is_err());
        config.storage_encryption = Some(encryption(dir.path()));
        config.database = dir.path().join("data/../secrets/financial.db");
        assert!(config.validate().is_err());
        config.database = PathBuf::from("relative.db");
        assert!(config.validate().is_err());
    }
}
