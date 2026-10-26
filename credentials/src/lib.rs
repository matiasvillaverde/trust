//! Native credential stores for local broker adapters and the legacy terminal client.
//!
//! Keep the existing service/username identifiers and native stores. Select the maintained
//! D-Bus backend explicitly instead of keyring's convenience feature and its retired zbus
//! dependencies. The service's mounted credentials do not use this module.

use std::sync::OnceLock;

pub use keyring_core::{Entry, Error, Result};

static INITIALIZED: OnceLock<Result<()>> = OnceLock::new();

/// Open an entry in the native secure store. Initialization failure never falls back to memory.
pub fn entry(service: &str, username: &str) -> Result<Entry> {
    if INITIALIZED.get_or_init(initialize).is_err() {
        return Err(Error::NoDefaultStore);
    }
    Entry::new(service, username)
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(unix, not(any(target_os = "ios", target_os = "android")))
))]
fn initialize() -> Result<()> {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new()?;
    #[cfg(target_os = "windows")]
    let store = windows_native_keyring_store::Store::new()?;
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    let store = dbus_secret_service_keyring_store::Store::new()?;
    keyring_core::set_default_store(store);
    Ok(())
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "windows",
    all(unix, not(any(target_os = "ios", target_os = "android")))
)))]
fn initialize() -> Result<()> {
    Err(Error::Invalid(
        "platform".into(),
        "No native credential store is available.".into(),
    ))
}
