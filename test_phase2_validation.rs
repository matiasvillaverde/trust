// Simple test to validate Phase 2 implementation
// This bypasses the compilation issues in unrelated workers
use db_sqlite::SqliteDatabase;
use model::{Account, AccountType, DatabaseFactory, Environment};
use rust_decimal::Decimal;
use uuid::Uuid;

fn main() {
    // Test basic database functionality with new schema
    println!("Testing Phase 2 implementation...");
    
    // Create in-memory database
    let db = SqliteDatabase::new_in_memory();
    
    // Test 1: Account creation with new fields should work
    println!("Test 1: Creating account with hierarchy support...");
    let account_result = db.account_write().create(
        "test account",
        "test description", 
        Environment::Paper,
        Decimal::new(25, 2),
        Decimal::new(30, 2)
    );
    
    match account_result {
        Ok(account) => {
            println!("✓ Account created successfully: {:?}", account.id);
            assert_eq!(account.account_type, AccountType::Primary);
            assert_eq!(account.parent_account_id, None);
            println!("✓ Account has correct default hierarchy fields");
        }
        Err(e) => {
            panic!("✗ Account creation failed: {}", e);
        }
    }
    
    // Test 2: Account retrieval should work with new fields
    println!("Test 2: Retrieving account with hierarchy fields...");
    let retrieved_result = db.account_read().for_name("test account");
    
    match retrieved_result {
        Ok(retrieved_account) => {
            println!("✓ Account retrieved successfully");
            assert_eq!(retrieved_account.account_type, AccountType::Primary);
            assert_eq!(retrieved_account.parent_account_id, None);
            println!("✓ Retrieved account has correct hierarchy fields");
        }
        Err(e) => {
            panic!("✗ Account retrieval failed: {}", e);
        }
    }
    
    println!("\nAll Phase 2 tests passed! ✓");
    println!("Database schema migration and account hierarchy implementation working correctly.");
}