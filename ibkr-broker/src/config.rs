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
            allow_insecure_tls: false,
        }
    }
}

impl ConnectionConfig {
    /// Create a new connection config.
    pub fn new(
        base_url: &str,
        allow_insecure_tls: bool,
    ) -> Result<Self, ConnectionConfigParseError> {
        Self::from_parts(base_url, allow_insecure_tls)
    }

    /// Read persisted settings for an account.
    pub fn read(environment: &Environment, account: &Account) -> keyring::Result<Self> {
        if let Some(config) = Self::from_env().map_err(|_| {
            keyring::Error::PlatformFailure(
                "Failed to parse IBKR connection config from environment"
                    .to_string()
                    .into(),
            )
        })? {
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
        Self::from_parts(&self.base_url, self.allow_insecure_tls).map_err(|_| {
            keyring::Error::PlatformFailure("Invalid IBKR connection config".to_string().into())
        })?;
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

    fn from_env() -> Result<Option<Self>, ConnectionConfigParseError> {
        let Ok(base_url) = std::env::var(ENV_URL) else {
            return Ok(None);
        };
        let allow_insecure_tls = std::env::var(ENV_ALLOW_INSECURE_TLS)
            .ok()
            .map(|value| parse_bool_flag(&value))
            .unwrap_or(false);
        Ok(Some(Self::from_parts(&base_url, allow_insecure_tls)?))
    }

    fn from_parts(
        base_url: &str,
        allow_insecure_tls: bool,
    ) -> Result<Self, ConnectionConfigParseError> {
        let normalized = normalize_base_url(base_url);
        if !is_supported_gateway_url(&normalized, allow_insecure_tls) {
            return Err(ConnectionConfigParseError);
        }
        Ok(Self {
            base_url: normalized,
            allow_insecure_tls,
        })
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

impl Display for ConnectionConfigParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "invalid IBKR connection configuration")
    }
}

impl std::error::Error for ConnectionConfigParseError {}

impl FromStr for ConnectionConfig {
    type Err = ConnectionConfigParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split_whitespace();
        let base_url = parts.next().ok_or(ConnectionConfigParseError)?;
        let allow_insecure_tls = parts.next().map(parse_bool_flag).unwrap_or(false);
        if parts.next().is_some() {
            return Err(ConnectionConfigParseError);
        }
        Self::from_parts(base_url, allow_insecure_tls)
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

fn is_supported_gateway_url(base_url: &str, allow_insecure_tls: bool) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return false;
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    let loopback = url
        .host_str()
        .map(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .map(|address| address.is_loopback())
                    .unwrap_or(false)
        })
        .unwrap_or(false);
    match url.scheme() {
        "https" => !allow_insecure_tls || loopback,
        "http" => loopback,
        _ => false,
    }
}

fn parse_bool_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::{ConnectionConfig, ConnectionConfigParseError, ENV_ALLOW_INSECURE_TLS, ENV_URL};
    use model::{Account, Environment};
    use std::str::FromStr;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvSnapshot {
        url: Option<String>,
        allow_insecure_tls: Option<String>,
    }

    impl EnvSnapshot {
        fn capture() -> Self {
            Self {
                url: std::env::var(ENV_URL).ok(),
                allow_insecure_tls: std::env::var(ENV_ALLOW_INSECURE_TLS).ok(),
            }
        }

        fn restore_var(name: &str, value: &Option<String>) {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }

    impl Drop for EnvSnapshot {
        fn drop(&mut self) {
            Self::restore_var(ENV_URL, &self.url);
            Self::restore_var(ENV_ALLOW_INSECURE_TLS, &self.allow_insecure_tls);
        }
    }

    #[test]
    fn config_default_matches_local_gateway_expectations() {
        let config = ConnectionConfig::default();
        assert_eq!(config.base_url, "https://localhost:5000/v1/api");
        assert!(!config.allow_insecure_tls);
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
    fn config_new_normalizes_display_debug_and_keychain_storage_format() {
        let config = ConnectionConfig::new(" https://ibkr.local/v1/api/// ", false)
            .expect("valid secure config");

        assert_eq!(config.base_url, "https://ibkr.local/v1/api");
        assert_eq!(
            config.to_string(),
            "https://ibkr.local/v1/api (allow_insecure_tls=false)"
        );
        assert_eq!(
            config.to_keychain_string(),
            "https://ibkr.local/v1/api false"
        );

        let debug = format!("{config:?}");
        assert!(debug.contains("https://ibkr.local/v1/api"));
        assert!(debug.contains("allow_insecure_tls"));
    }

    #[test]
    fn config_parser_rejects_missing_base_url() {
        assert_eq!(
            ConnectionConfig::from_str("").unwrap_err(),
            ConnectionConfigParseError
        );
    }

    #[test]
    fn config_parser_rejects_extra_tokens_and_accepts_bool_aliases() {
        assert_eq!(
            ConnectionConfig::from_str("https://ibkr.local false trailing").unwrap_err(),
            ConnectionConfigParseError
        );
        assert!(
            ConnectionConfig::from_str("https://localhost:5000/v1/api yes")
                .unwrap()
                .allow_insecure_tls
        );
        assert!(
            ConnectionConfig::from_str("https://localhost:5000/v1/api ON")
                .unwrap()
                .allow_insecure_tls
        );
        assert!(
            !ConnectionConfig::from_str("https://ibkr.local off")
                .unwrap()
                .allow_insecure_tls
        );
        assert!(
            !ConnectionConfig::from_str("https://ibkr.local")
                .unwrap()
                .allow_insecure_tls
        );
        assert_eq!(
            ConnectionConfig::from_str("file:///etc/passwd false").unwrap_err(),
            ConnectionConfigParseError
        );
        assert_eq!(
            ConnectionConfig::from_str("http://gateway.example/v1/api false").unwrap_err(),
            ConnectionConfigParseError
        );
        let credential_url = format!(
            "https://{}:{}@gateway.example/v1/api false",
            "test-user", "test-password"
        );
        assert_eq!(
            ConnectionConfig::from_str(&credential_url).unwrap_err(),
            ConnectionConfigParseError
        );
        assert_eq!(
            ConnectionConfig::from_str("https://gateway.example/v1/api?mode=paper false")
                .unwrap_err(),
            ConnectionConfigParseError
        );
        assert_eq!(
            ConnectionConfig::from_str("https://gateway.example/v1/api true").unwrap_err(),
            ConnectionConfigParseError
        );
    }

    #[test]
    fn config_read_prefers_environment_override_without_keychain() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _snapshot = EnvSnapshot::capture();
        std::env::set_var(ENV_URL, " https://env-ibkr.local/v1/api/// ");
        std::env::set_var(ENV_ALLOW_INSECURE_TLS, "no");

        let account = Account {
            name: "ibkr-env-override".to_string(),
            environment: Environment::Paper,
            ..Account::default()
        };
        let config = ConnectionConfig::read(&Environment::Paper, &account)
            .expect("environment override should not use keychain");

        assert_eq!(config.base_url, "https://env-ibkr.local/v1/api");
        assert!(!config.allow_insecure_tls);
    }

    #[test]
    fn config_read_rejects_invalid_environment_url_without_keychain() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _snapshot = EnvSnapshot::capture();
        std::env::set_var(ENV_URL, "file:///etc/passwd");
        std::env::remove_var(ENV_ALLOW_INSECURE_TLS);

        let account = Account {
            name: "ibkr-invalid-env".to_string(),
            environment: Environment::Paper,
            ..Account::default()
        };
        let error = ConnectionConfig::read(&Environment::Paper, &account)
            .expect_err("invalid environment URL should fail before keychain");

        assert!(error.to_string().contains("environment"));
    }
}
