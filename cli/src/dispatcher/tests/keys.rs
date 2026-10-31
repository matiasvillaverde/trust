#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::KeysCommandBuilder;
use clap::Arg;
use clap::Command;

#[test]
fn test_protected_keyword_lifecycle_set_rotate_and_delete_requires_valid_confirmation() {
    let _guard = env_guard();
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
    crate::protected_keyword::delete().expect("reset protected keyword");
    let mut dispatcher = test_dispatcher();

    let missing_value = Command::new("keys")
        .arg(Arg::new("value").long("value"))
        .arg(Arg::new("confirm-protected").long("confirm-protected"))
        .get_matches_from(["keys"]);
    let missing_error =
        crate::dispatcher::keys::set_protected_keyword(&mut dispatcher.trust, &missing_value)
            .expect_err("missing value should fail");
    assert!(missing_error.to_string().contains("missing_value"));
    crate::dispatcher::keys::show_protected_keyword(&mut dispatcher.trust);

    let blank_initial = KeysCommandBuilder::new()
        .protected_set()
        .build()
        .get_matches_from(["keys", "protected-set", "--value", "   "]);
    let blank_initial_error = crate::dispatcher::keys::set_protected_keyword(
        &mut dispatcher.trust,
        blank_initial
            .subcommand_matches("protected-set")
            .expect("protected-set matches"),
    )
    .expect_err("blank initial keyword should fail in storage");
    assert!(blank_initial_error
        .to_string()
        .contains("protected_keyword_store_failed"));

    let initial_set = KeysCommandBuilder::new()
        .protected_set()
        .build()
        .get_matches_from(["keys", "protected-set", "--value", "first-secret"]);
    crate::dispatcher::keys::set_protected_keyword(
        &mut dispatcher.trust,
        initial_set
            .subcommand_matches("protected-set")
            .expect("protected-set matches"),
    )
    .expect("initial set should succeed");
    assert_eq!(
        crate::protected_keyword::read_expected().expect("keyword should be stored"),
        "first-secret"
    );
    crate::dispatcher::keys::show_protected_keyword(&mut dispatcher.trust);

    let rotate_missing_confirm = KeysCommandBuilder::new()
        .protected_set()
        .build()
        .get_matches_from(["keys", "protected-set", "--value", "second-secret"]);
    let rotate_missing_error = crate::dispatcher::keys::set_protected_keyword(
        &mut dispatcher.trust,
        rotate_missing_confirm
            .subcommand_matches("protected-set")
            .expect("protected-set matches"),
    )
    .expect_err("rotation without confirmation should fail");
    assert!(rotate_missing_error
        .to_string()
        .contains("protected_keyword_required"));

    let rotate_wrong_confirm = KeysCommandBuilder::new()
        .protected_set()
        .build()
        .get_matches_from([
            "keys",
            "protected-set",
            "--value",
            "second-secret",
            "--confirm-protected",
            "wrong",
        ]);
    let rotate_wrong_error = crate::dispatcher::keys::set_protected_keyword(
        &mut dispatcher.trust,
        rotate_wrong_confirm
            .subcommand_matches("protected-set")
            .expect("protected-set matches"),
    )
    .expect_err("rotation with wrong confirmation should fail");
    assert!(rotate_wrong_error
        .to_string()
        .contains("protected_keyword_invalid"));

    let rotate_blank_value = KeysCommandBuilder::new()
        .protected_set()
        .build()
        .get_matches_from([
            "keys",
            "protected-set",
            "--value",
            "   ",
            "--confirm-protected",
            "first-secret",
        ]);
    let rotate_blank_error = crate::dispatcher::keys::set_protected_keyword(
        &mut dispatcher.trust,
        rotate_blank_value
            .subcommand_matches("protected-set")
            .expect("protected-set matches"),
    )
    .expect_err("blank rotation keyword should fail in storage");
    assert!(rotate_blank_error
        .to_string()
        .contains("protected_keyword_store_failed"));
    assert_eq!(
        crate::protected_keyword::read_expected().expect("keyword after failed blank rotation"),
        "first-secret"
    );

    let rotate_valid = KeysCommandBuilder::new()
        .protected_set()
        .build()
        .get_matches_from([
            "keys",
            "protected-set",
            "--value",
            "second-secret",
            "--confirm-protected",
            "first-secret",
        ]);
    crate::dispatcher::keys::set_protected_keyword(
        &mut dispatcher.trust,
        rotate_valid
            .subcommand_matches("protected-set")
            .expect("protected-set matches"),
    )
    .expect("rotation with valid confirmation should succeed");
    assert_eq!(
        crate::protected_keyword::read_expected().expect("keyword after rotation"),
        "second-secret"
    );

    let delete_wrong_confirm = KeysCommandBuilder::new()
        .protected_delete()
        .build()
        .get_matches_from(["keys", "protected-delete", "--confirm-protected", "wrong"]);
    let delete_wrong_error = crate::dispatcher::keys::delete_protected_keyword(
        &mut dispatcher.trust,
        delete_wrong_confirm
            .subcommand_matches("protected-delete")
            .expect("protected-delete matches"),
    )
    .expect_err("deletion with wrong confirmation should fail");
    assert!(delete_wrong_error
        .to_string()
        .contains("protected_keyword_invalid"));

    let delete_valid = KeysCommandBuilder::new()
        .protected_delete()
        .build()
        .get_matches_from([
            "keys",
            "protected-delete",
            "--confirm-protected",
            "second-secret",
        ]);
    crate::dispatcher::keys::delete_protected_keyword(
        &mut dispatcher.trust,
        delete_valid
            .subcommand_matches("protected-delete")
            .expect("protected-delete matches"),
    )
    .expect("deletion with valid confirmation should succeed");
    assert!(
        crate::protected_keyword::read_expected().is_err(),
        "keyword should be deleted"
    );

    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "0");
    let delete_store_failure = KeysCommandBuilder::new()
        .protected_delete()
        .build()
        .get_matches_from(["keys", "protected-delete", "--confirm-protected", "secret"]);
    let delete_store_error = crate::dispatcher::keys::delete_protected_keyword(
        &mut dispatcher.trust,
        delete_store_failure
            .subcommand_matches("protected-delete")
            .expect("protected-delete matches"),
    )
    .expect_err("keychain deletion failures should surface");
    assert!(delete_store_error
        .to_string()
        .contains("protected_keyword_delete_failed"));
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");

    crate::protected_keyword::delete().expect("cleanup protected keyword");
    std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
}
