use keyring::Entry;
use model::{Account, Environment};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

const DEFAULT_GATEWAY_URL: &str = "https://localhost:5000/v1/api";
const ENV_URL: &str = "TRUST_IBKR_URL";
const ENV_ALLOW_INSECURE_TLS: &str = "TRUST_IBKR_ALLOW_INSECURE_TLS";

/// Connection settings for the local or hosted IBKR Client Portal endpoint.
#[derive(Clone, PartialEq, Eq)]
pub struct ConnectionConfig {
    /// Base URL for the gateway or API endpoint.
    pub base_url: String,
    /// Whether TLS certificate validation should be relaxed.
    pub allow_insecure_tls: bool,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_GATEWAY_URL.to_string(),
            allow_insecure_tls: true,
        }
    }
}

impl ConnectionConfig {
    /// Create a new connection config.
    pub fn new(base_url: &str, allow_insecure_tls: bool) -> Self {
        Self {
            base_url: normalize_base_url(base_url),
            allow_insecure_tls,
        }
    }

    /// Read persisted settings for an account.
    pub fn read(environment: &Environment, account: &Account) -> keyring::Result<Self> {
        if let Some(config) = Self::from_env() {
            return Ok(config);
        }

        let entry = entry(environment, &account.name)?;
        match entry.get_password() {
            Ok(raw) => ConnectionConfig::from_str(&raw).map_err(|_| {
                keyring::Error::PlatformFailure(
                    "Failed to parse IBKR connection config from keychain"
                        .to_string()
                        .into(),
                )
            }),
            Err(keyring::Error::NoEntry) => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    /// Persist settings for an account.
    pub fn store(self, environment: &Environment, account: &Account) -> keyring::Result<Self> {
        let entry = entry(environment, &account.name)?;
        entry.set_password(&self.to_keychain_string())?;
        Ok(self)
    }

    /// Remove persisted settings for an account.
    pub fn delete(environment: &Environment, account: &Account) -> keyring::Result<()> {
        let entry = entry(environment, &account.name)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn to_keychain_string(&self) -> String {
        format!("{} {}", self.base_url, self.allow_insecure_tls)
    }

    fn from_env() -> Option<Self> {
        let base_url = std::env::var(ENV_URL).ok()?;
        let allow_insecure_tls = std::env::var(ENV_ALLOW_INSECURE_TLS)
            .ok()
            .map(|value| parse_bool_flag(&value))
            .unwrap_or(true);
        Some(Self::new(&base_url, allow_insecure_tls))
    }
}

impl std::fmt::Debug for ConnectionConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionConfig")
            .field("base_url", &self.base_url)
            .field("allow_insecure_tls", &self.allow_insecure_tls)
            .finish()
    }
}

impl Display for ConnectionConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (allow_insecure_tls={})",
            self.base_url, self.allow_insecure_tls
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionConfigParseError;

impl FromStr for ConnectionConfig {
    type Err = ConnectionConfigParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split_whitespace();
        let base_url = parts.next().ok_or(ConnectionConfigParseError)?;
        let allow_insecure_tls = parts.next().map(parse_bool_flag).unwrap_or(true);
        Ok(Self::new(base_url, allow_insecure_tls))
    }
}

fn entry(environment: &Environment, account_name: &str) -> keyring::Result<Entry> {
    Entry::new(
        &format!("trust-ibkr:{account_name}"),
        environment.to_string().as_str(),
    )
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

fn parse_bool_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::{ConnectionConfig, ConnectionConfigParseError};
    use std::str::FromStr;

    #[test]
    fn config_default_matches_local_gateway_expectations() {
        let config = ConnectionConfig::default();
        assert_eq!(config.base_url, "https://localhost:5000/v1/api");
        assert!(config.allow_insecure_tls);
    }

    #[test]
    fn config_roundtrips_via_display_format() {
        let config = ConnectionConfig::from_str("https://ibkr.local/v1/api false").unwrap();
        assert_eq!(config.base_url, "https://ibkr.local/v1/api");
        assert!(!config.allow_insecure_tls);
        assert_eq!(
            ConnectionConfig::from_str("https://ibkr.local/v1/api false").unwrap(),
            config
        );
    }

    #[test]
    fn config_parser_rejects_missing_base_url() {
        assert_eq!(
            ConnectionConfig::from_str("").unwrap_err(),
            ConnectionConfigParseError
        );
    }
}
