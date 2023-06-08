use keyring::{Entry, Result};
use std::fmt::{self, Display, Formatter};

pub struct Keys {
    pub key_id: String,
    pub secret: String,
    pub url: String,
}

impl Keys {
    pub fn new(key_id: &str, secret: &str, url: &str) -> Keys {
        Keys {
            key_id: key_id.to_string(),
            secret: secret.to_string(),
            url: url.to_string(),
        }
    }
}

impl Keys {
    pub fn read(environment: &Environment) -> Result<Keys> {
        let keys = get_keys(environment)?;
        Ok(keys)
    }

    pub fn store(self, environment: &Environment) -> Result<Keys> {
        let keys = store_keys(self, environment)?;
        Ok(keys)
    }

    pub fn delete(environment: &Environment) -> Result<()> {
        delete_keys(environment)?;
        Ok(())
    }
}

fn delete_keys(environment: &Environment) -> Result<()> {
    delete(EntryType::KeyId, environment)?;
    delete(EntryType::Secret, environment)?;
    delete(EntryType::Url, environment)?;
    Ok(())
}

fn store_keys(keys: Keys, environment: &Environment) -> Result<Keys> {
    store(EntryType::KeyId, environment, keys.key_id.as_str())?;
    store(EntryType::Secret, environment, keys.secret.as_str())?;
    store(EntryType::Url, environment, keys.url.as_str())?;
    Ok(keys)
}

fn get_keys(environment: &Environment) -> Result<Keys> {
    let key_id = get(EntryType::KeyId, environment)?;
    let secret = get(EntryType::Secret, environment)?;
    let url = get(EntryType::Url, environment)?;
    Ok(Keys {
        key_id,
        secret,
        url,
    })
}

fn store(entry: EntryType, environment: &Environment, value: &str) -> Result<()> {
    let entry = Entry::new(environment.to_string().as_str(), entry.to_string().as_str())?;
    entry.set_password(value)?;
    Ok(())
}

fn get(entry: EntryType, environment: &Environment) -> Result<String> {
    let entry = Entry::new(environment.to_string().as_str(), entry.to_string().as_str())?;
    let password = entry.get_password()?;
    Ok(password)
}

fn delete(entry: EntryType, environment: &Environment) -> Result<()> {
    let entry = Entry::new(environment.to_string().as_str(), entry.to_string().as_str())?;
    let result = entry.delete_password()?;
    Ok(result)
}

#[derive(Debug, Clone, Copy)]
pub enum Environment {
    Paper,
    Live,
}

impl Environment {
    pub fn all() -> Vec<Environment> {
        vec![Environment::Paper, Environment::Live]
    }
}

pub enum EntryType {
    KeyId,
    Secret,
    Url,
}

impl Display for EntryType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            EntryType::KeyId => write!(f, "KeyId"),
            EntryType::Secret => write!(f, "Secret"),
            EntryType::Url => write!(f, "Url"),
        }
    }
}

impl Display for Environment {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            Environment::Paper => write!(f, "paper"),
            Environment::Live => write!(f, "live"),
        }
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
