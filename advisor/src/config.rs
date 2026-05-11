use crate::error::AdvisorError;
use keyring::Entry;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

const SERVICE: &str = "trust-advisor";
const CALENDAR_API_KEY: &str = "calendar_api_key";
const CLAUDE_API_KEY: &str = "claude_api_key";
const CALENDAR_PROVIDER: &str = "calendar_provider";

/// Calendar data provider used by catalyst scanning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CalendarProvider {
    /// Financial Modeling Prep calendar endpoint.
    Fmp,
    /// Polygon.io calendar endpoint.
    Polygon,
    /// Calendar-driven features are disabled.
    #[default]
    None,
}

impl CalendarProvider {
    /// Stable storage key for the provider.
    pub fn as_str(self) -> &'static str {
        match self {
            CalendarProvider::Fmp => "fmp",
            CalendarProvider::Polygon => "polygon",
            CalendarProvider::None => "none",
        }
    }
}

impl Display for CalendarProvider {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CalendarProvider {
    type Err = AdvisorError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "fmp" | "financial-modeling-prep" | "financialmodelingprep" => Ok(Self::Fmp),
            "polygon" | "polygon.io" => Ok(Self::Polygon),
            "none" | "off" | "disabled" => Ok(Self::None),
            _ => Err(AdvisorError::InvalidCalendarProvider {
                value: value.to_string(),
            }),
        }
    }
}

/// Redacted advisor configuration status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisorConfig {
    /// Configured calendar provider.
    pub calendar_provider: CalendarProvider,
    /// Whether a calendar API key is present.
    pub calendar_api_key_configured: bool,
    /// Whether a Claude API key is present.
    pub claude_api_key_configured: bool,
}

impl AdvisorConfig {
    /// Read advisor configuration status from the system keychain.
    pub fn read() -> Result<Self, AdvisorError> {
        let store = KeychainSecretStore;
        read_with_store(&store)
    }

    /// Human-safe display value for calendar API key state.
    pub fn calendar_api_key_display(&self) -> &'static str {
        redacted_key_state(self.calendar_api_key_configured)
    }

    /// Human-safe display value for Claude API key state.
    pub fn claude_api_key_display(&self) -> &'static str {
        redacted_key_state(self.claude_api_key_configured)
    }
}

impl Default for AdvisorConfig {
    fn default() -> Self {
        Self {
            calendar_provider: CalendarProvider::None,
            calendar_api_key_configured: false,
            claude_api_key_configured: false,
        }
    }
}

/// Calendar provider credentials used by HTTP-backed catalyst scans.
#[derive(Clone, PartialEq, Eq)]
pub struct CalendarCredentials {
    /// Configured calendar provider.
    provider: CalendarProvider,
    api_key: Option<String>,
}

impl CalendarCredentials {
    /// Build explicit calendar credentials.
    pub fn new(provider: CalendarProvider, api_key: Option<String>) -> Self {
        let normalized_key = api_key.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        Self {
            provider,
            api_key: normalized_key,
        }
    }

    /// Read calendar credentials from the system keychain.
    pub fn read() -> Result<Self, AdvisorError> {
        let store = KeychainSecretStore;
        read_calendar_credentials_with_store(&store)
    }

    /// Configured calendar provider.
    pub fn provider(&self) -> CalendarProvider {
        self.provider
    }

    /// Calendar API key, if one is configured.
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Returns true when a nonblank calendar API key is available.
    pub fn has_api_key(&self) -> bool {
        self.api_key().is_some()
    }
}

impl Default for CalendarCredentials {
    fn default() -> Self {
        Self {
            provider: CalendarProvider::None,
            api_key: None,
        }
    }
}

impl fmt::Debug for CalendarCredentials {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("CalendarCredentials")
            .field("provider", &self.provider)
            .field("api_key_configured", &self.has_api_key())
            .finish()
    }
}

/// Partial advisor configuration update.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdvisorConfigUpdate {
    /// Calendar provider to persist.
    pub calendar_provider: Option<CalendarProvider>,
    /// Calendar API key to persist.
    pub calendar_api_key: Option<String>,
    /// Claude API key to persist.
    pub claude_api_key: Option<String>,
}

impl AdvisorConfigUpdate {
    /// Returns true when the update contains no changes.
    pub fn is_empty(&self) -> bool {
        self.calendar_provider.is_none()
            && self.calendar_api_key.is_none()
            && self.claude_api_key.is_none()
    }

    /// Persist this update to the system keychain and return redacted status.
    pub fn apply(&self) -> Result<AdvisorConfig, AdvisorError> {
        let store = KeychainSecretStore;
        apply_update_with_store(&store, self)
    }
}

trait SecretStore {
    fn read(&self, name: &'static str) -> Result<Option<String>, AdvisorError>;
    fn write(&self, name: &'static str, value: &str) -> Result<(), AdvisorError>;
}

#[derive(Debug, Clone, Copy)]
struct KeychainSecretStore;

impl SecretStore for KeychainSecretStore {
    fn read(&self, name: &'static str) -> Result<Option<String>, AdvisorError> {
        let entry = Entry::new(SERVICE, name)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, name: &'static str, value: &str) -> Result<(), AdvisorError> {
        let entry = Entry::new(SERVICE, name)?;
        entry.set_password(value)?;
        Ok(())
    }
}

fn apply_update_with_store(
    store: &dyn SecretStore,
    update: &AdvisorConfigUpdate,
) -> Result<AdvisorConfig, AdvisorError> {
    if let Some(provider) = update.calendar_provider {
        store.write(CALENDAR_PROVIDER, provider.as_str())?;
    }
    if let Some(key) = update.calendar_api_key.as_deref() {
        store_secret(store, CALENDAR_API_KEY, key, "calendar_api_key")?;
    }
    if let Some(key) = update.claude_api_key.as_deref() {
        store_secret(store, CLAUDE_API_KEY, key, "claude_api_key")?;
    }
    read_with_store(store)
}

fn read_with_store(store: &dyn SecretStore) -> Result<AdvisorConfig, AdvisorError> {
    let credentials = read_calendar_credentials_with_store(store)?;
    Ok(AdvisorConfig {
        calendar_provider: credentials.provider(),
        calendar_api_key_configured: credentials.has_api_key(),
        claude_api_key_configured: has_nonblank_secret(store, CLAUDE_API_KEY)?,
    })
}

fn read_calendar_credentials_with_store(
    store: &dyn SecretStore,
) -> Result<CalendarCredentials, AdvisorError> {
    let provider = match store.read(CALENDAR_PROVIDER)? {
        Some(value) => CalendarProvider::from_str(&value)?,
        None => CalendarProvider::None,
    };
    Ok(CalendarCredentials::new(
        provider,
        read_nonblank_secret(store, CALENDAR_API_KEY)?,
    ))
}

fn store_secret(
    store: &dyn SecretStore,
    name: &'static str,
    value: &str,
    field: &'static str,
) -> Result<(), AdvisorError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AdvisorError::BlankValue { field });
    }
    store.write(name, trimmed)
}

fn has_nonblank_secret(store: &dyn SecretStore, name: &'static str) -> Result<bool, AdvisorError> {
    Ok(read_nonblank_secret(store, name)?.is_some())
}

fn read_nonblank_secret(
    store: &dyn SecretStore,
    name: &'static str,
) -> Result<Option<String>, AdvisorError> {
    Ok(store.read(name)?.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }))
}

fn redacted_key_state(is_configured: bool) -> &'static str {
    if is_configured {
        "[REDACTED]"
    } else {
        "missing"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemorySecretStore {
        values: Mutex<HashMap<&'static str, String>>,
    }

    impl SecretStore for MemorySecretStore {
        fn read(&self, name: &'static str) -> Result<Option<String>, AdvisorError> {
            let values = self.values.lock().unwrap();
            Ok(values.get(name).cloned())
        }

        fn write(&self, name: &'static str, value: &str) -> Result<(), AdvisorError> {
            let mut values = self.values.lock().unwrap();
            values.insert(name, value.to_string());
            Ok(())
        }
    }

    #[test]
    fn calendar_provider_display_and_parse_roundtrip() {
        for provider in [
            CalendarProvider::Fmp,
            CalendarProvider::Polygon,
            CalendarProvider::None,
        ] {
            let parsed = CalendarProvider::from_str(provider.as_str()).unwrap();
            assert_eq!(parsed, provider);
            assert_eq!(provider.to_string(), provider.as_str());
        }
        assert_eq!(
            CalendarProvider::from_str("FMP").unwrap(),
            CalendarProvider::Fmp
        );
        assert_eq!(
            CalendarProvider::from_str("polygon.io").unwrap(),
            CalendarProvider::Polygon
        );
        assert!(CalendarProvider::from_str("unknown").is_err());
    }

    #[test]
    fn advisor_config_update_stores_redacted_status() {
        let store = MemorySecretStore::default();
        let update = AdvisorConfigUpdate {
            calendar_provider: Some(CalendarProvider::Polygon),
            calendar_api_key: Some(" calendar-secret ".to_string()),
            claude_api_key: Some("claude-secret".to_string()),
        };

        let config = apply_update_with_store(&store, &update).unwrap();

        assert_eq!(config.calendar_provider, CalendarProvider::Polygon);
        assert!(config.calendar_api_key_configured);
        assert!(config.claude_api_key_configured);
        assert_eq!(config.calendar_api_key_display(), "[REDACTED]");
        assert_eq!(config.claude_api_key_display(), "[REDACTED]");
        assert_eq!(
            store.read(CALENDAR_API_KEY).unwrap(),
            Some("calendar-secret".to_string())
        );
        let credentials = read_calendar_credentials_with_store(&store).unwrap();
        assert_eq!(credentials.provider(), CalendarProvider::Polygon);
        assert_eq!(credentials.api_key(), Some("calendar-secret"));
        assert!(!format!("{credentials:?}").contains("calendar-secret"));
    }

    #[test]
    fn read_with_store_defaults_missing_values() {
        let store = MemorySecretStore::default();

        let config = read_with_store(&store).unwrap();

        assert_eq!(config, AdvisorConfig::default());
        assert_eq!(config.calendar_api_key_display(), "missing");
        assert_eq!(config.claude_api_key_display(), "missing");
    }

    #[test]
    fn update_rejects_blank_secrets_and_bad_provider_rows() {
        let store = MemorySecretStore::default();
        let update = AdvisorConfigUpdate {
            calendar_api_key: Some(" \t ".to_string()),
            ..AdvisorConfigUpdate::default()
        };
        assert!(apply_update_with_store(&store, &update)
            .unwrap_err()
            .to_string()
            .contains("calendar_api_key"));

        store.write(CALENDAR_PROVIDER, "bad-provider").unwrap();
        assert!(read_with_store(&store)
            .unwrap_err()
            .to_string()
            .contains("unsupported calendar provider"));
    }
}
