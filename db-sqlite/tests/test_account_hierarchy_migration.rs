use db_sqlite::SqliteDatabase;
use model::{Account, AccountType, DatabaseFactory};
use rust_decimal::Decimal;
use uuid::Uuid;

#[test]
fn test_account_hierarchy_fields_exist_after_migration() {
    // Given: A fresh in-memory database with migrations applied
    let db = SqliteDatabase::new_in_memory();

    // When: We create an account with hierarchy fields
    let account = Account {
        id: Uuid::new_v4(),
        account_type: AccountType::Primary,
        parent_account_id: None,
        name: "Main Trading".to_string(),
        description: "Primary trading account".to_string(),
        environment: model::Environment::Paper,
        taxes_percentage: Decimal::new(25, 2),
        earnings_percentage: Decimal::new(30, 2),
        ..Account::default()
    };

    // Then: The account should be created successfully with new fields
    let result = db.account_write().create(
        &account.name,
        &account.description,
        account.environment,
        account.taxes_percentage,
        account.earnings_percentage,
    );
    assert!(
        result.is_ok(),
        "Account creation should succeed after migration"
    );

    // And: We should be able to retrieve the account with hierarchy fields
    let created_account = result.unwrap();
    let retrieved = db.account_read().id(created_account.id);
    assert!(retrieved.is_ok(), "Account retrieval should succeed");

    let retrieved_account = retrieved.unwrap();
    assert_eq!(retrieved_account.account_type, AccountType::Primary);
    assert_eq!(retrieved_account.parent_account_id, None);
}

#[test]
fn test_account_hierarchy_child_parent_relationship() {
    // Given: A fresh database with parent account
    let db = SqliteDatabase::new_in_memory();

    let parent = Account {
        id: Uuid::new_v4(),
        account_type: AccountType::Primary,
        parent_account_id: None,
        name: "Main Trading".to_string(),
        description: "Primary trading account".to_string(),
        environment: model::Environment::Paper,
        taxes_percentage: Decimal::new(25, 2),
        earnings_percentage: Decimal::new(30, 2),
        ..Account::default()
    };

    // When: We create parent and child accounts
    let _created_parent = db
        .account_write()
        .create_with_hierarchy(
            &parent.name,
            &parent.description,
            parent.environment,
            parent.taxes_percentage,
            parent.earnings_percentage,
            AccountType::Primary,
            None,
        )
        .unwrap();

    // Then: Child account creation should succeed
    let result = db.account_write().create_with_hierarchy(
        "Earnings Account",
        "Personal earnings allocation",
        model::Environment::Paper,
        Decimal::new(0, 2),
        Decimal::new(0, 2),
        AccountType::Earnings,
        Some(_created_parent.id),
    );
    assert!(result.is_ok(), "Child account creation should succeed");

    // And: Child should maintain parent relationship
    let created_child = result.unwrap();
    let retrieved_child = db.account_read().id(created_child.id).unwrap();
    assert_eq!(retrieved_child.account_type, AccountType::Earnings);
    assert_eq!(retrieved_child.parent_account_id, Some(_created_parent.id));
}

#[test]
fn test_distribution_tables_exist_after_migration() {
    // Given: A fresh database with migrations applied
    let db = SqliteDatabase::new_in_memory();
    let account = db
        .account_write()
        .create(
            "distribution-main",
            "distribution-main",
            model::Environment::Paper,
            Decimal::new(25, 2),
            Decimal::new(30, 2),
        )
        .unwrap();

    let created = db
        .distribution_write()
        .create_or_update(
            account.id,
            Decimal::new(40, 2),
            Decimal::new(30, 2),
            Decimal::new(30, 2),
            Decimal::new(100, 0),
            "test-lock-hash",
        )
        .unwrap();

    let retrieved = db.distribution_read().for_account(account.id).unwrap();
    assert_eq!(created.account_id, retrieved.account_id);
    assert_eq!(
        retrieved.configuration_password_hash,
        "test-lock-hash".to_string()
    );
}

#[test]
fn test_existing_accounts_get_default_values_after_migration() {
    // This test ensures backward compatibility
    // Given: An existing account (simulated by creating without new fields)
    let db = SqliteDatabase::new_in_memory();

    // When: We retrieve an existing account after migration
    let account = Account {
        id: Uuid::new_v4(),
        name: "Legacy Account".to_string(),
        description: "Account created before hierarchy support".to_string(),
        environment: model::Environment::Paper,
        taxes_percentage: Decimal::new(25, 2),
        earnings_percentage: Decimal::new(30, 2),
        // New fields should get defaults from the database layer
        account_type: AccountType::Primary, // This will be set by database layer
        parent_account_id: None,            // This will be set by database layer
        ..Account::default()
    };

    // Then: Account operations should work normally
    let result = db.account_write().create(
        &account.name,
        &account.description,
        account.environment,
        account.taxes_percentage,
        account.earnings_percentage,
    );
    assert!(result.is_ok(), "Legacy account should work after migration");

    let created_account = result.unwrap();
    let retrieved = db.account_read().id(created_account.id).unwrap();
    // Default values should be applied
    assert_eq!(retrieved.account_type, AccountType::Primary);
    assert_eq!(retrieved.parent_account_id, None);
}
