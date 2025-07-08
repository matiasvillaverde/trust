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
#[derive(Debug)]
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

impl Display for Keys {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.url, self.key_id, self.secret)
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
        entry.set_password(self.to_string().as_str())?;
        Ok(self)
    }

    /// Delete API keys from keychain
    pub fn delete(environment: &Environment, account_name: &str) -> keyring::Result<()> {
        let entry = Entry::new(account_name, environment.to_string().as_str())?;
        entry.get_credential();
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
}
