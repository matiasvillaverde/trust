//! Private boundary for native connection settings; tests never open the user's keychain.
use trust_credentials::Result;

pub(super) trait ConnectionStore {
    fn read(&self, service: &str, user: &str) -> Result<String>;
    fn write(&self, service: &str, user: &str, value: &str) -> Result<()>;
    fn delete(&self, service: &str, user: &str) -> Result<()>;
}

pub(super) struct NativeStore;
impl ConnectionStore for NativeStore {
    fn read(&self, service: &str, user: &str) -> Result<String> {
        trust_credentials::entry(service, user)?.get_password()
    }
    fn write(&self, service: &str, user: &str, value: &str) -> Result<()> {
        trust_credentials::entry(service, user)?.set_password(value)
    }
    fn delete(&self, service: &str, user: &str) -> Result<()> {
        trust_credentials::entry(service, user)?.delete_credential()
    }
}

#[cfg(test)]
mod tests;
