use super::*;
use crate::config::ConnectionConfig;
use model::{Account, Environment};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use trust_credentials::Error;

#[derive(Default)]
struct MemoryStore {
    values: RefCell<BTreeMap<(String, String), String>>,
    fail: Cell<bool>,
}
impl MemoryStore {
    fn available(&self) -> Result<()> {
        if self.fail.get() {
            Err(Error::PlatformFailure("store unavailable".into()))
        } else {
            Ok(())
        }
    }
}
impl ConnectionStore for MemoryStore {
    fn read(&self, service: &str, user: &str) -> Result<String> {
        self.available()?;
        self.values
            .borrow()
            .get(&(service.into(), user.into()))
            .cloned()
            .ok_or(Error::NoEntry)
    }
    fn write(&self, service: &str, user: &str, value: &str) -> Result<()> {
        self.available()?;
        self.values
            .borrow_mut()
            .insert((service.into(), user.into()), value.into());
        Ok(())
    }
    fn delete(&self, service: &str, user: &str) -> Result<()> {
        self.available()?;
        self.values
            .borrow_mut()
            .remove(&(service.into(), user.into()))
            .ok_or(Error::NoEntry)?;
        Ok(())
    }
}

#[test]
fn connection_settings_persist_with_account_and_environment_isolation() {
    let store = MemoryStore::default();
    let alpha = Account {
        name: "alpha".into(),
        ..Account::default()
    };
    let beta = Account {
        name: "beta".into(),
        ..Account::default()
    };
    let paper = ConnectionConfig::new("https://localhost:5000/v1/api/", true).unwrap();
    let live = ConnectionConfig::new("https://gateway.example.test/v1/api", false).unwrap();
    paper
        .clone()
        .store_in(&store, &Environment::Paper, &alpha)
        .unwrap();
    live.clone()
        .store_in(&store, &Environment::Live, &alpha)
        .unwrap();
    live.clone()
        .store_in(&store, &Environment::Paper, &beta)
        .unwrap();
    assert_eq!(
        store
            .values
            .borrow()
            .get(&("trust-ibkr:alpha".into(), "paper".into()))
            .unwrap(),
        "https://localhost:5000/v1/api true"
    );
    assert_eq!(
        ConnectionConfig::read_from(&store, &Environment::Paper, &alpha).unwrap(),
        paper
    );
    assert_eq!(
        ConnectionConfig::read_from(&store, &Environment::Live, &alpha).unwrap(),
        live
    );
    ConnectionConfig::delete_from(&store, &Environment::Paper, &alpha).unwrap();
    ConnectionConfig::delete_from(&store, &Environment::Paper, &alpha).unwrap();
    assert_eq!(
        ConnectionConfig::read_from(&store, &Environment::Paper, &alpha).unwrap(),
        ConnectionConfig::default()
    );
    assert_eq!(
        ConnectionConfig::read_from(&store, &Environment::Live, &alpha).unwrap(),
        live
    );
    assert_eq!(
        ConnectionConfig::read_from(&store, &Environment::Paper, &beta).unwrap(),
        live
    );
    assert_eq!(store.values.borrow().len(), 2);
}

#[test]
fn invalid_connection_changes_preserve_the_previous_settings() {
    let store = MemoryStore::default();
    let account = Account {
        name: "alpha".into(),
        ..Account::default()
    };
    let good = ConnectionConfig::default();
    good.clone()
        .store_in(&store, &Environment::Paper, &account)
        .unwrap();
    for (url, insecure) in [
        ("http://remote.example.test", false),
        ("https://remote.example.test", true),
        ("https://invalid-user@localhost", false),
    ] {
        let invalid = ConnectionConfig {
            base_url: url.into(),
            allow_insecure_tls: insecure,
        };
        assert!(invalid
            .store_in(&store, &Environment::Paper, &account)
            .is_err());
        assert_eq!(
            ConnectionConfig::read_from(&store, &Environment::Paper, &account).unwrap(),
            good
        );
    }
}

#[test]
fn unavailable_or_corrupt_settings_are_errors_not_default_connections() {
    let store = MemoryStore::default();
    let account = Account {
        name: "alpha".into(),
        ..Account::default()
    };
    store.values.borrow_mut().insert(
        ("trust-ibkr:alpha".into(), "paper".into()),
        "corrupt secret marker".into(),
    );
    let corrupt = ConnectionConfig::read_from(&store, &Environment::Paper, &account).unwrap_err();
    assert!(!corrupt.to_string().contains("corrupt secret marker"));
    store.fail.set(true);
    assert!(ConnectionConfig::read_from(&store, &Environment::Paper, &account).is_err());
    assert!(ConnectionConfig::default()
        .store_in(&store, &Environment::Paper, &account)
        .is_err());
    assert!(ConnectionConfig::delete_from(&store, &Environment::Paper, &account).is_err());
    assert_eq!(store.values.borrow().len(), 1);
}
