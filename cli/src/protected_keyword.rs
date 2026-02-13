use keyring::Entry;
use std::error::Error;

const SERVICE: &str = "trust-cli";
const USERNAME: &str = "protected-mutation-keyword";
const ENV_OVERRIDE: &str = "TRUST_PROTECTED_KEYWORD_EXPECTED";

pub fn read_expected() -> Result<String, Box<dyn Error>> {
    if let Ok(value) = std::env::var(ENV_OVERRIDE) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let entry = Entry::new(SERVICE, USERNAME)?;
    let password = entry.get_password()?;
    let trimmed = password.trim();
    if trimmed.is_empty() {
        return Err("stored protected keyword is empty".into());
    }
    Ok(trimmed.to_string())
}

pub fn store(value: &str) -> Result<(), Box<dyn Error>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("protected keyword cannot be empty".into());
    }
    let entry = Entry::new(SERVICE, USERNAME)?;
    entry.set_password(trimmed)?;
    Ok(())
}

pub fn delete() -> Result<(), Box<dyn Error>> {
    let entry = Entry::new(SERVICE, USERNAME)?;
    let _ = entry.delete_credential();
    Ok(())
}
