use apca::ApiInfo;
use keyring::Entry;
use model::{Account, Environment};
use std::error::Error;
use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

pub fn read_api_key(env: &Environment, account: &Account) -> Result<ApiInfo, Box<dyn Error>> {
    let keys = Keys::read(env, &account.name)?;
    let info = ApiInfo::from_parts(keys.url, keys.key_id, keys.secret)?;
    Ok(info)
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
        let url = split.next().unwrap_or_default().to_string();
        let key_id = split.next().unwrap_or_default().to_string();
        let secret = split.next().unwrap_or_default().to_string();
        Ok(Keys::new(key_id.as_str(), secret.as_str(), url.as_str()))
    }
}

impl Keys {
    /// Serialize keys for keychain storage (not redacted).
    fn to_keychain_string(&self) -> String {
        format!("{} {} {}", self.url, self.key_id, self.secret)
    }

    /// Read API keys from keychain
    pub fn read(environment: &Environment, account_name: &str) -> keyring::Result<Keys> {
        let entry = Entry::new(account_name, environment.to_string().as_str())?;
        let password = entry.get_password()?;
        let keys = Keys::from_str(password.as_str()).map_err(|_| {
            keyring::Error::PlatformFailure("Failed to parse Keys from string".to_string().into())
        })?;
        Ok(keys)
    }

    /// Store API keys in keychain
    pub fn store(self, environment: &Environment, account_name: &str) -> keyring::Result<Keys> {
        let entry = Entry::new(account_name, environment.to_string().as_str())?;
        entry.set_password(self.to_keychain_string().as_str())?;
        Ok(self)
    }

    /// Delete API keys from keychain
    pub fn delete(environment: &Environment, account_name: &str) -> keyring::Result<()> {
        let entry = Entry::new(account_name, environment.to_string().as_str())?;
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
}
