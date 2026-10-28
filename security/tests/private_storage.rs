#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use std::{
    io::Write,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
};
use trust_security::{private_file, read_secret, DatabaseKey};

#[test]
fn secret_permissions_links_special_files_and_bounds_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("secret");
    private_file(&path, true)
        .unwrap()
        .write_all(b"sensitive-fixture")
        .unwrap();
    assert!(read_secret(&path, 32).is_ok());
    assert!(read_secret(&path, 2).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read_secret(&path, 32).is_err());
    assert!(private_file(&path, false).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let linked = dir.path().join("linked");
    symlink(&path, &linked).unwrap();
    assert!(read_secret(&linked, 32).is_err());
    assert!(private_file(&linked, false).is_err());
    let alias = dir.path().join("hardlink");
    std::fs::hard_link(&path, &alias).unwrap();
    assert!(read_secret(&path, 32).is_err());
    assert!(read_secret(&alias, 32).is_err());
    assert!(read_secret(dir.path(), 32).is_err());
    let fifo = dir.path().join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    assert!(read_secret(&fifo, 32).is_err());
    assert!(private_file(&fifo, false).is_err());
    let config = dir.path().join("config.json");
    std::fs::write(&config, b"{}").unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(trust_security::read_public_config(&config, 16).is_ok());
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(trust_security::read_public_config(&config, 16).is_err());
}

#[test]
fn private_directories_and_keys_never_overwrite_or_accept_bad_formats() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("private/key");
    let key = DatabaseKey::generate().unwrap();
    key.write_new(&path).unwrap();
    assert!(key.same_as(&DatabaseKey::load(&path).unwrap()));
    assert!(!key.same_as(&DatabaseKey::generate().unwrap()));
    assert!(key.write_new(&path).is_err());
    assert_eq!(format!("{key:?}"), "DatabaseKey([REDACTED])");
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        std::fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    for invalid in [
        "",
        "password",
        "x'1234'",
        &"g".repeat(64),
        &"a".repeat(63),
        &"a".repeat(65),
    ] {
        assert!(DatabaseKey::from_hex(invalid).is_err());
    }
    let public = dir.path().join("public");
    std::fs::create_dir(&public).unwrap();
    std::fs::set_permissions(&public, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(private_file(&public.join("key"), true).is_err());
    let link = dir.path().join("linked-directory");
    symlink(path.parent().unwrap(), &link).unwrap();
    assert!(read_secret(&link.join("key"), 66).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn production_accepts_ram_mounted_secrets() {
    let dir = tempfile::tempdir_in("/dev/shm").unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("key");
    DatabaseKey::generate().unwrap().write_new(&path).unwrap();
    trust_security::require_memory_file(&path).unwrap();
}

#[test]
fn native_backend_requires_patched_stable_sqlcipher_four() {
    for version in [
        "",
        "3.53.4",
        "4.14.0 community",
        "4.18.1",
        "4.19",
        "4.19.0-beta",
        "5.0.0",
    ] {
        assert!(
            !trust_security::supported_sqlcipher_version(version),
            "{version}"
        );
    }
    assert!(trust_security::supported_sqlcipher_version(
        "4.19.0 community"
    ));
    assert!(trust_security::supported_sqlcipher_version("4.20.1"));
}
