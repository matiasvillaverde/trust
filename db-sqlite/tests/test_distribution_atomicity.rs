use db_sqlite::SqliteDatabase;
use model::{
    AccountType, Currency, DatabaseFactory, DistributionExecutionLeg, DistributionExecutionPlan,
    Environment, TransactionCategory,
};
use rust_decimal_macros::dec;
use uuid::Uuid;

fn create_hierarchy(
    db: &SqliteDatabase,
    prefix: &str,
) -> (model::Account, model::Account, model::Account) {
    let source = db
        .account_write()
        .create_with_hierarchy(
            &format!("{prefix}-primary"),
            &format!("{prefix}-primary"),
            Environment::Paper,
            dec!(0),
            dec!(0),
            AccountType::Primary,
            None,
        )
        .unwrap();

    let child_a = db
        .account_write()
        .create_with_hierarchy(
            &format!("{prefix}-a"),
            &format!("{prefix}-a"),
            Environment::Paper,
            dec!(0),
            dec!(0),
            AccountType::Earnings,
            Some(source.id),
        )
        .unwrap();

    let child_b = db
        .account_write()
        .create_with_hierarchy(
            &format!("{prefix}-b"),
            &format!("{prefix}-b"),
            Environment::Paper,
            dec!(0),
            dec!(0),
            AccountType::TaxReserve,
            Some(source.id),
        )
        .unwrap();

    (source, child_a, child_b)
}

#[test]
fn test_execute_distribution_plan_atomic_rolls_back_on_midway_failure() {
    let db = SqliteDatabase::new_in_memory();
    let (source, child_a, child_b) = create_hierarchy(&db, "atomic-rollback");

    // Force a unique-constraint failure on the second leg deposit by reusing the same tx id.
    let duplicated_deposit_id = Uuid::new_v4();

    let plan = DistributionExecutionPlan {
        source_account_id: source.id,
        currency: Currency::USD,
        trade_id: None,
        original_amount: dec!(1000),
        distribution_date: chrono::Utc::now().naive_utc(),
        legs: vec![
            DistributionExecutionLeg {
                to_account_id: child_a.id,
                amount: dec!(400),
                withdrawal_category: TransactionCategory::Withdrawal,
                deposit_category: TransactionCategory::Deposit,
                forced_withdrawal_tx_id: Some(Uuid::new_v4()),
                forced_deposit_tx_id: Some(duplicated_deposit_id),
            },
            DistributionExecutionLeg {
                to_account_id: child_b.id,
                amount: dec!(600),
                withdrawal_category: TransactionCategory::Withdrawal,
                deposit_category: TransactionCategory::Deposit,
                forced_withdrawal_tx_id: Some(Uuid::new_v4()),
                forced_deposit_tx_id: Some(duplicated_deposit_id), // duplicates first leg deposit id
            },
        ],
        earnings_amount: Some(dec!(400)),
        tax_amount: Some(dec!(600)),
        reinvestment_amount: None,
    };

    let err = db
        .distribution_write()
        .execute_distribution_plan_atomic(&plan)
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("UNIQUE") || msg.contains("unique") || msg.contains("constraint"),
        "expected unique constraint failure, got: {msg}"
    );

    // Ensure full rollback: no transactions, no history.
    let tx_source = db
        .transaction_read()
        .all_transactions(source.id, &Currency::USD)
        .unwrap();
    let tx_a = db
        .transaction_read()
        .all_transactions(child_a.id, &Currency::USD)
        .unwrap();
    let tx_b = db
        .transaction_read()
        .all_transactions(child_b.id, &Currency::USD)
        .unwrap();
    assert!(tx_source.is_empty());
    assert!(tx_a.is_empty());
    assert!(tx_b.is_empty());

    let history = db
        .distribution_read()
        .history_for_account(source.id)
        .unwrap();
    assert!(history.is_empty());
}

#[test]
fn test_execute_distribution_plan_atomic_happy_path_writes_transfers_and_history() {
    let db = SqliteDatabase::new_in_memory();
    let (source, child_a, child_b) = create_hierarchy(&db, "atomic-happy");

    let plan = DistributionExecutionPlan {
        source_account_id: source.id,
        currency: Currency::USD,
        trade_id: Some(Uuid::new_v4()),
        original_amount: dec!(1000),
        distribution_date: chrono::Utc::now().naive_utc(),
        legs: vec![
            DistributionExecutionLeg {
                to_account_id: child_a.id,
                amount: dec!(400),
                withdrawal_category: TransactionCategory::Withdrawal,
                deposit_category: TransactionCategory::Deposit,
                forced_withdrawal_tx_id: None,
                forced_deposit_tx_id: None,
            },
            DistributionExecutionLeg {
                to_account_id: child_b.id,
                amount: dec!(600),
                withdrawal_category: TransactionCategory::Withdrawal,
                deposit_category: TransactionCategory::Deposit,
                forced_withdrawal_tx_id: None,
                forced_deposit_tx_id: None,
            },
        ],
        earnings_amount: Some(dec!(400)),
        tax_amount: Some(dec!(600)),
        reinvestment_amount: None,
    };

    let deposit_ids = db
        .distribution_write()
        .execute_distribution_plan_atomic(&plan)
        .unwrap();
    assert_eq!(deposit_ids.len(), 2);

    let history = db
        .distribution_read()
        .history_for_account(source.id)
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].original_amount, dec!(1000));
}
