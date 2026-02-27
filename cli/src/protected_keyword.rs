#[cfg(not(test))]
use keyring::Entry;
use std::error::Error;
use std::sync::{Mutex, OnceLock};

#[cfg(not(test))]
const SERVICE: &str = "trust-cli";
#[cfg(not(test))]
const USERNAME: &str = "protected-mutation-keyword";
const ENV_OVERRIDE: &str = "TRUST_PROTECTED_KEYWORD_EXPECTED";
const DISABLE_KEYCHAIN_ENV: &str = "TRUST_DISABLE_KEYCHAIN";
static IN_MEMORY_PROTECTED_KEYWORD: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static CREDENTIAL_STORE: OnceLock<Mutex<Box<dyn CredentialStore + Send>>> = OnceLock::new();

trait CredentialStore {
    fn get(&self) -> Result<String, Box<dyn Error>>;
    fn set(&self, value: &str) -> Result<(), Box<dyn Error>>;
    fn delete(&self) -> Result<(), Box<dyn Error>>;
}

struct KeyringCredentialStore;

impl CredentialStore for KeyringCredentialStore {
    #[cfg(not(test))]
    fn get(&self) -> Result<String, Box<dyn Error>> {
        let entry = Entry::new(SERVICE, USERNAME)?;
        let password = entry.get_password()?;
        Ok(password)
    }

    #[cfg(test)]
    fn get(&self) -> Result<String, Box<dyn Error>> {
        Err("keyring credential store is disabled in tests".into())
    }

    #[cfg(not(test))]
    fn set(&self, value: &str) -> Result<(), Box<dyn Error>> {
        let entry = Entry::new(SERVICE, USERNAME)?;
        entry.set_password(value)?;
        Ok(())
    }

    #[cfg(test)]
    fn set(&self, _value: &str) -> Result<(), Box<dyn Error>> {
        Err("keyring credential store is disabled in tests".into())
    }

    #[cfg(not(test))]
    fn delete(&self) -> Result<(), Box<dyn Error>> {
        let entry = Entry::new(SERVICE, USERNAME)?;
        let _ = entry.delete_credential();
        Ok(())
    }

    #[cfg(test)]
    fn delete(&self) -> Result<(), Box<dyn Error>> {
        Err("keyring credential store is disabled in tests".into())
    }
}

fn keychain_disabled() -> bool {
    if cfg!(test) {
        return !matches!(
            std::env::var(DISABLE_KEYCHAIN_ENV)
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            Some("0" | "false" | "no" | "off")
        );
    }

    matches!(
        std::env::var(DISABLE_KEYCHAIN_ENV)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn in_memory_protected_keyword() -> &'static Mutex<Option<String>> {
    IN_MEMORY_PROTECTED_KEYWORD.get_or_init(|| Mutex::new(None))
}

fn credential_store() -> &'static Mutex<Box<dyn CredentialStore + Send>> {
    CREDENTIAL_STORE.get_or_init(|| Mutex::new(Box::new(KeyringCredentialStore)))
}

#[cfg(test)]
fn with_credential_store(
    store: Box<dyn CredentialStore + Send>,
) -> Box<dyn CredentialStore + Send> {
    let mut guard = credential_store().lock().expect("credential store lock");
    std::mem::replace(&mut *guard, store)
}

pub fn read_expected() -> Result<String, Box<dyn Error>> {
    if let Ok(value) = std::env::var(ENV_OVERRIDE) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    if keychain_disabled() {
        let value = in_memory_protected_keyword()
            .lock()
            .map_err(|_| "failed to read in-memory protected keyword")?
            .clone();

        if let Some(value) = value {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        return Err("protected keyword not configured".into());
    }

    let password = credential_store()
        .lock()
        .map_err(|_| "failed to lock credential store")?
        .get()?;
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

    if keychain_disabled() {
        let mut state = in_memory_protected_keyword()
            .lock()
            .map_err(|_| "failed to write in-memory protected keyword")?;
        *state = Some(trimmed.to_string());
        return Ok(());
    }

    credential_store()
        .lock()
        .map_err(|_| "failed to lock credential store")?
        .set(trimmed)?;
    Ok(())
}

pub fn delete() -> Result<(), Box<dyn Error>> {
    if keychain_disabled() {
        let mut state = in_memory_protected_keyword()
            .lock()
            .map_err(|_| "failed to clear in-memory protected keyword")?;
        *state = None;
        return Ok(());
    }

    credential_store()
        .lock()
        .map_err(|_| "failed to lock credential store")?
        .delete()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        delete, keychain_disabled, read_expected, store, with_credential_store, CredentialStore,
        KeyringCredentialStore, DISABLE_KEYCHAIN_ENV, ENV_OVERRIDE,
    };
    use std::env;
    use std::sync::{Mutex, OnceLock};

    #[derive(Default)]
    struct StubStore {
        value: Mutex<Option<String>>,
        fail_get: bool,
    }

    impl CredentialStore for StubStore {
        fn get(&self) -> Result<String, Box<dyn std::error::Error>> {
            if self.fail_get {
                return Err("store get failed".into());
            }
            self.value
                .lock()
                .map_err(|_| "stub lock failed".to_string())?
                .clone()
                .ok_or_else(|| "missing".into())
        }

        fn set(&self, value: &str) -> Result<(), Box<dyn std::error::Error>> {
            *self
                .value
                .lock()
                .map_err(|_| "stub lock failed".to_string())? = Some(value.to_string());
            Ok(())
        }

        fn delete(&self) -> Result<(), Box<dyn std::error::Error>> {
            *self
                .value
                .lock()
                .map_err(|_| "stub lock failed".to_string())? = None;
            Ok(())
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn read_expected_prefers_non_empty_env_override() {
        let _guard = env_lock().lock().expect("lock");
        env::set_var(DISABLE_KEYCHAIN_ENV, "1");
        delete().expect("clear in-memory value");
        env::set_var(ENV_OVERRIDE, "  secret  ");
        let value = read_expected().expect("env override should be used");
        assert_eq!(value, "secret");
        env::remove_var(ENV_OVERRIDE);
        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }

    #[test]
    fn store_rejects_empty_values() {
        let _guard = env_lock().lock().expect("lock");
        let result = store("   ");
        assert!(result.is_err());
        assert_eq!(
            result
                .expect_err("empty protected keyword should fail")
                .to_string(),
            "protected keyword cannot be empty"
        );
    }

    #[test]
    fn read_expected_does_not_touch_keychain_without_override_in_tests() {
        let _guard = env_lock().lock().expect("lock");
        delete().expect("clear in-memory value");
        env::remove_var(ENV_OVERRIDE);
        let error = read_expected().expect_err("tests should not access keychain");
        assert_eq!(error.to_string(), "protected keyword not configured");
    }

    #[test]
    fn store_read_delete_round_trip_with_disabled_keychain() {
        let _guard = env_lock().lock().expect("lock");
        env::set_var(DISABLE_KEYCHAIN_ENV, "1");
        delete().expect("clear in-memory value");

        store("  abc123  ").expect("store in memory");
        assert_eq!(read_expected().expect("read in memory"), "abc123");

        delete().expect("delete in memory");
        assert!(read_expected().is_err());

        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }

    #[test]
    fn read_store_delete_use_credential_store_when_keychain_enabled() {
        let _guard = env_lock().lock().expect("lock");
        env::remove_var(ENV_OVERRIDE);
        env::set_var(DISABLE_KEYCHAIN_ENV, "0");

        let previous = with_credential_store(Box::new(StubStore::default()));
        store("abc").expect("store should use credential store");
        assert_eq!(
            read_expected().expect("read should use credential store"),
            "abc"
        );
        delete().expect("delete should use credential store");
        assert!(read_expected().is_err());
        let _ = with_credential_store(previous);

        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }

    #[test]
    fn read_expected_surfaces_credential_store_error() {
        let _guard = env_lock().lock().expect("lock");
        env::remove_var(ENV_OVERRIDE);
        env::set_var(DISABLE_KEYCHAIN_ENV, "0");

        let previous = with_credential_store(Box::new(StubStore {
            value: Mutex::new(None),
            fail_get: true,
        }));
        let error = read_expected().expect_err("credential store error should surface");
        assert!(error.to_string().contains("store get failed"));
        let _ = with_credential_store(previous);

        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }

    #[test]
    fn read_expected_rejects_blank_value_from_enabled_credential_store() {
        let _guard = env_lock().lock().expect("lock");
        env::remove_var(ENV_OVERRIDE);
        env::set_var(DISABLE_KEYCHAIN_ENV, "0");

        let previous = with_credential_store(Box::new(StubStore {
            value: Mutex::new(Some("   ".to_string())),
            fail_get: false,
        }));
        let error = read_expected().expect_err("blank stored values should be rejected");
        assert_eq!(error.to_string(), "stored protected keyword is empty");
        let _ = with_credential_store(previous);

        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }

    #[test]
    fn env_override_takes_precedence_over_enabled_credential_store_value() {
        let _guard = env_lock().lock().expect("lock");
        env::set_var(ENV_OVERRIDE, " override-secret ");
        env::set_var(DISABLE_KEYCHAIN_ENV, "0");

        let previous = with_credential_store(Box::new(StubStore {
            value: Mutex::new(Some("store-secret".to_string())),
            fail_get: false,
        }));
        let value = read_expected().expect("env override should win");
        assert_eq!(value, "override-secret");
        let _ = with_credential_store(previous);

        env::remove_var(ENV_OVERRIDE);
        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }

    #[test]
    fn store_trims_value_before_writing_enabled_credential_store() {
        let _guard = env_lock().lock().expect("lock");
        env::remove_var(ENV_OVERRIDE);
        env::set_var(DISABLE_KEYCHAIN_ENV, "0");

        let previous = with_credential_store(Box::new(StubStore::default()));
        store("  trimmed-secret  ").expect("store should trim input before write");
        let value = read_expected().expect("read should return trimmed value");
        assert_eq!(value, "trimmed-secret");
        let _ = with_credential_store(previous);

        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }

    #[test]
    fn keychain_disabled_in_tests_defaults_to_true_except_explicit_false_values() {
        let _guard = env_lock().lock().expect("lock");

        env::remove_var(DISABLE_KEYCHAIN_ENV);
        assert!(keychain_disabled());

        for value in ["0", "false", "no", "off"] {
            env::set_var(DISABLE_KEYCHAIN_ENV, value);
            assert!(
                !keychain_disabled(),
                "value `{value}` should enable keychain"
            );
        }

        for value in ["1", "true", "yes", "on", "anything-else"] {
            env::set_var(DISABLE_KEYCHAIN_ENV, value);
            assert!(
                keychain_disabled(),
                "value `{value}` should disable keychain"
            );
        }

        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }

    #[test]
    fn read_expected_ignores_blank_env_override_and_uses_in_memory_value() {
        let _guard = env_lock().lock().expect("lock");
        env::set_var(DISABLE_KEYCHAIN_ENV, "1");
        delete().expect("clear in-memory");
        store("memory-secret").expect("store in-memory value");

        env::set_var(ENV_OVERRIDE, "   ");
        let value = read_expected().expect("blank override should fall back to memory");
        assert_eq!(value, "memory-secret");

        env::remove_var(ENV_OVERRIDE);
        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }

    #[test]
    fn default_keyring_store_is_never_used_successfully_in_tests() {
        let _guard = env_lock().lock().expect("lock");
        env::remove_var(ENV_OVERRIDE);
        env::set_var(DISABLE_KEYCHAIN_ENV, "0");
        let previous = with_credential_store(Box::new(KeyringCredentialStore));

        let store_error = store("abc").expect_err("test keyring store should error");
        assert!(store_error.to_string().contains("disabled in tests"));

        let read_error = read_expected().expect_err("test keyring read should error");
        assert!(read_error.to_string().contains("disabled in tests"));

        let delete_error = delete().expect_err("test keyring delete should error");
        assert!(delete_error.to_string().contains("disabled in tests"));

        let _ = with_credential_store(previous);
        env::remove_var(DISABLE_KEYCHAIN_ENV);
    }
}
