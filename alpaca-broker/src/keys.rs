use apca::ApiInfo;
use model::{Account, Environment};
use std::error::Error;
use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

pub fn read_api_key(env: &Environment, account: &Account) -> Result<ApiInfo, Box<dyn Error>> {
    let keys = if let Some(path) = std::env::var_os("TRUST_ALPACA_CREDENTIALS_FILE") {
        let bytes = trust_security::read_secret(std::path::Path::new(&path), 65536)?;
        file_credentials(&bytes, env, account.id)?
    } else {
        Keys::read(env, &account.name)?
    };
    let info = ApiInfo::from_parts(keys.url, keys.key_id, keys.secret)?;
    Ok(info)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FileCredentials {
    environment: String,
    key_id: String,
    secret: String,
}

impl Drop for FileCredentials {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.key_id.zeroize();
        self.secret.zeroize();
    }
}

fn file_credentials(
    bytes: &[u8],
    environment: &Environment,
    account_id: uuid::Uuid,
) -> Result<Keys, Box<dyn Error>> {
    let entries: std::collections::BTreeMap<uuid::Uuid, FileCredentials> =
        serde_json::from_slice(bytes).map_err(|_| "Invalid Alpaca credential file")?;
    let entry = entries
        .get(&account_id)
        .ok_or("No Alpaca credentials for this account")?;
    if entry.environment != environment.to_string()
        || entry.key_id.trim().is_empty()
        || entry.secret.trim().is_empty()
    {
        return Err("Alpaca credentials do not match account environment or are empty".into());
    }
    // Secret files cannot redirect keys to an arbitrary endpoint.
    let url = match environment {
        Environment::Paper => "https://paper-api.alpaca.markets",
        Environment::Live => "https://api.alpaca.markets",
    };
    Ok(Keys::new(&entry.key_id, &entry.secret, url))
}

/// API keys for connecting to Alpaca broker
pub struct Keys {
    /// The API key ID
    pub key_id: String,
    /// The API secret key
    pub secret: String,
    /// The base URL for the API
    pub url: String,
}

impl Keys {
    /// Create new API keys
    pub fn new(key_id: &str, secret: &str, url: &str) -> Keys {
        Keys {
            key_id: key_id.to_string(),
            secret: secret.to_string(),
            url: url.to_string(),
        }
    }
}

impl fmt::Debug for Keys {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keys")
            .field("key_id", &self.key_id)
            .field("secret", &"[REDACTED]")
            .field("url", &self.url)
            .finish()
    }
}

impl Display for Keys {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} [REDACTED]", self.url, self.key_id)
    }
}

#[derive(PartialEq, Debug)]
pub struct KeysParseError;
impl FromStr for Keys {
    type Err = KeysParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split_whitespace();
        let url = split.next().ok_or(KeysParseError)?.to_string();
        let key_id = split.next().ok_or(KeysParseError)?.to_string();
        let secret = split.next().ok_or(KeysParseError)?.to_string();
        if split.next().is_some() {
            return Err(KeysParseError);
        }
        Ok(Keys::new(key_id.as_str(), secret.as_str(), url.as_str()))
    }
}

impl Keys {
    /// Serialize keys for keychain storage (not redacted).
    fn to_keychain_string(&self) -> String {
        format!("{} {} {}", self.url, self.key_id, self.secret)
    }

    /// Read API keys from keychain
    pub fn read(environment: &Environment, account_name: &str) -> trust_credentials::Result<Keys> {
        let entry = trust_credentials::entry(account_name, environment.to_string().as_str())?;
        let password = zeroize::Zeroizing::new(entry.get_password()?);
        let keys = Keys::from_str(password.as_str()).map_err(|_| {
            trust_credentials::Error::PlatformFailure(
                "Failed to parse Keys from string".to_string().into(),
            )
        })?;
        Ok(keys)
    }

    /// Store API keys in keychain
    pub fn store(
        self,
        environment: &Environment,
        account_name: &str,
    ) -> trust_credentials::Result<Keys> {
        let entry = trust_credentials::entry(account_name, environment.to_string().as_str())?;
        entry.set_password(self.to_keychain_string().as_str())?;
        Ok(self)
    }

    /// Delete API keys from keychain
    pub fn delete(environment: &Environment, account_name: &str) -> trust_credentials::Result<()> {
        let entry = trust_credentials::entry(account_name, environment.to_string().as_str())?;
        entry.delete_credential()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keys_new() {
        let key_id = "my_key_id";
        let secret = "my_secret";
        let url = "https://example.com";
        let keys = Keys::new(key_id, secret, url);

        assert_eq!(keys.key_id, key_id);
        assert_eq!(keys.secret, secret);
        assert_eq!(keys.url, url);
    }

    #[test]
    fn test_display_redacts_secret() {
        let keys = Keys::new("my_key_id", "super_secret_value", "https://example.com");
        let display = format!("{keys}");
        assert!(!display.contains("super_secret_value"));
        assert!(display.contains("[REDACTED]"));
        assert!(display.contains("my_key_id"));
    }

    #[test]
    fn test_debug_redacts_secret() {
        let keys = Keys::new("my_key_id", "super_secret_value", "https://example.com");
        let debug = format!("{keys:?}");
        assert!(!debug.contains("super_secret_value"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn test_keychain_string_roundtrips_with_from_str() {
        let keys = Keys::new("my_key_id", "my_secret", "https://example.com");
        let serialized = keys.to_keychain_string();
        let parsed = Keys::from_str(&serialized).unwrap();
        assert_eq!(parsed.key_id, "my_key_id");
        assert_eq!(parsed.secret, "my_secret");
        assert_eq!(parsed.url, "https://example.com");
    }

    // ---------------------------------------------------------------
    // Security tests
    // ---------------------------------------------------------------

    /// Parsing an empty string should return Err, not empty Keys.
    #[test]
    fn from_str_empty_string_should_error() {
        assert!(Keys::from_str("").is_err());
    }

    /// Parsing only a URL (missing key_id and secret) should return Err.
    #[test]
    fn from_str_partial_input_should_error() {
        assert!(Keys::from_str("https://example.com").is_err());
    }

    /// Parsing URL + key_id but no secret should return Err.
    #[test]
    fn from_str_missing_secret_should_error() {
        assert!(Keys::from_str("https://example.com my_key_id").is_err());
    }

    /// Extra tokens beyond the expected three should return Err.
    #[test]
    fn from_str_extra_tokens_should_error() {
        assert!(Keys::from_str("https://example.com key_id secret EXTRA_DATA more_stuff").is_err());
    }
}

#[cfg(test)]
mod file_credential_tests {
    use super::*;
    #[test]
    fn mounted_credentials_bind_account_and_environment_without_custom_endpoints() {
        let account = uuid::Uuid::new_v4();
        let bytes=serde_json::to_vec(&serde_json::json!({account.to_string():{"environment":"paper","key_id":"test-only","secret":"test-only"}})).unwrap();
        let keys = file_credentials(&bytes, &Environment::Paper, account).unwrap();
        assert_eq!(keys.url, "https://paper-api.alpaca.markets");
        assert!(file_credentials(&bytes, &Environment::Live, account).is_err());
        assert!(file_credentials(&bytes, &Environment::Paper, uuid::Uuid::new_v4()).is_err());
        let invalid=serde_json::to_vec(&serde_json::json!({account.to_string():{"environment":"paper","key_id":"test-only","secret":"test-only","url":"https://attacker.invalid"}})).unwrap();
        assert!(file_credentials(&invalid, &Environment::Paper, account).is_err());
    }
}
