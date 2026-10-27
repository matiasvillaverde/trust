use crate::auth::Grant;
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
    pub grants: BTreeMap<String, Grant>,
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
        let bytes = std::fs::read(path)
            .map_err(|_| ServiceError::invalid("Cannot read the service configuration."))?;
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
        Ok(())
    }

    pub fn resource(&self) -> String {
        format!("{}/mcp", self.public_url)
    }
}
