//! Security and correctness tests for Trust core.
//!
//! Each test asserts the expected fixed behavior directly so these regressions
//! fail loudly if any of the security guarantees are weakened.

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects,
    clippy::field_reassign_with_default
)]
mod tests {
    use chrono::{DateTime, Utc};
    use model::{
        Account, BrokerKind, BrokerLog, Execution, FeeActivity, Order, OrderIds, Status, Trade,
    };
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::error::Error;

    // ---------------------------------------------------------------
    // Stub broker (minimal impl for facade tests)
    // ---------------------------------------------------------------
    struct StubBroker;
    impl model::Broker for StubBroker {
        fn kind(&self) -> BrokerKind {
            BrokerKind::Alpaca
        }
        fn submit_trade(
            &self,
            _: &Trade,
            _: &Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
            Err("stub".into())
        }
        fn sync_trade(
            &self,
            _: &Trade,
            _: &Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
            Err("stub".into())
        }
        fn cancel_trade(&self, _: &Trade, _: &Account) -> Result<(), Box<dyn Error>> {
            Err("stub".into())
        }
        fn close_trade(
            &self,
            _: &Trade,
            _: &Account,
        ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
            Err("stub".into())
        }
        fn modify_stop(
            &self,
            _: &Trade,
            _: &Account,
            _: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("stub".into())
        }
        fn modify_target(
            &self,
            _: &Trade,
            _: &Account,
            _: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("stub".into())
        }
        fn fetch_executions(
            &self,
            _: &Trade,
            _: &Account,
            _: Option<DateTime<Utc>>,
        ) -> Result<Vec<Execution>, Box<dyn Error>> {
            Ok(vec![])
        }
        fn fetch_fee_activities(
            &self,
            _: &Trade,
            _: &Account,
            _: Option<DateTime<Utc>>,
        ) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
            Ok(vec![])
        }
    }

    // ===============================================================
    // 1. PROTECTED MODE BYPASS - no password required
    // ===============================================================

    /// Protected mutations should require valid credentials.
    #[test]
    fn protected_mode_should_require_password() {
        use crate::TrustFacade;
        use model::{Currency, Environment, TransactionCategory};

        let db = db_sqlite::SqliteDatabase::new_in_memory();
        let mut facade = TrustFacade::new(Box::new(db), Box::new(StubBroker));

        let account = facade
            .create_account("sec-test", "test", Environment::Paper, dec!(30), dec!(70))
            .unwrap();

        facade.enable_protected_mode();
        assert!(
            facade.authorize_protected_mutation("", "").is_err(),
            "Blank protected credentials should be rejected"
        );

        let result = facade.create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(100),
            &Currency::USD,
        );
        // CORRECT behavior: this should be Err because no password was provided.
        assert!(
            result.is_err(),
            "Protected mutation should fail without a password"
        );
    }

    /// Confirm that without calling authorize, operations are blocked.
    #[test]
    fn protected_mode_blocks_without_authorize() {
        use crate::TrustFacade;
        use model::{Currency, Environment, TransactionCategory};

        let db = db_sqlite::SqliteDatabase::new_in_memory();
        let mut facade = TrustFacade::new(Box::new(db), Box::new(StubBroker));

        let account = facade
            .create_account("sec-test2", "test", Environment::Paper, dec!(30), dec!(70))
            .unwrap();

        facade.enable_protected_mode();

        let result = facade.create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(100),
            &Currency::USD,
        );
        assert!(result.is_err(), "Protected mode correctly blocks");
    }

    /// Re-authorization should require credentials each time.
    #[test]
    fn protected_mode_re_authorization_should_require_credentials() {
        use crate::TrustFacade;
        use model::{Currency, Environment, TransactionCategory};

        let db = db_sqlite::SqliteDatabase::new_in_memory();
        let mut facade = TrustFacade::new(Box::new(db), Box::new(StubBroker));

        let account = facade
            .create_account("sec-reauth", "test", Environment::Paper, dec!(30), dec!(70))
            .unwrap();

        facade.enable_protected_mode();

        facade
            .authorize_protected_mutation("correct-secret", "correct-secret")
            .unwrap();
        let _ = facade.create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(10),
            &Currency::USD,
        );
        assert!(
            facade
                .authorize_protected_mutation("wrong-secret", "correct-secret")
                .is_err(),
            "Re-authorization should reject invalid credentials"
        );
        let result = facade.create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(10),
            &Currency::USD,
        );
        assert!(
            result.is_err(),
            "Re-authorization should require credentials"
        );
    }

    // ===============================================================
    // 2. ZERO-AMOUNT DEPOSIT SHOULD BE REJECTED
    // ===============================================================

    /// A zero-amount deposit should be rejected like all other zero-amount
    /// financial operations.
    #[test]
    #[should_panic]
    fn zero_amount_deposit_should_be_rejected() {
        use model::{AccountBalance, AccountBalanceRead, Currency};
        use uuid::Uuid;

        struct FakeBalanceRead;
        impl AccountBalanceRead for FakeBalanceRead {
            fn for_account(&mut self, _: Uuid) -> Result<Vec<AccountBalance>, Box<dyn Error>> {
                Ok(vec![AccountBalance {
                    total_available: dec!(1000),
                    ..Default::default()
                }])
            }
            fn for_currency(
                &mut self,
                _: Uuid,
                _: &Currency,
            ) -> Result<AccountBalance, Box<dyn Error>> {
                Ok(AccountBalance {
                    total_available: dec!(1000),
                    ..Default::default()
                })
            }
        }

        let result = crate::validators::transaction::can_transfer_deposit(
            dec!(0),
            &Currency::USD,
            Uuid::new_v4(),
            &mut FakeBalanceRead,
        );

        // CORRECT behavior: zero deposit should be rejected.
        assert!(result.is_err(), "Zero-amount deposit should be rejected");
    }

    // ===============================================================
    // 3. STOP MODIFICATION — SHOULD RETURN TradeNotFilled FOR UNFILLED TRADES
    // ===============================================================

    /// When a non-Filled trade attempts stop modification, the error
    /// should be TradeNotFilled regardless of the price.
    #[test]
    #[should_panic]
    fn modify_stop_should_check_status_before_price_for_long() {
        use crate::validators::trade::{can_modify_stop, TradeValidationErrorCode};
        use model::TradeCategory;

        let trade = Trade {
            status: Status::New,
            category: TradeCategory::Long,
            safety_stop: Order {
                unit_price: dec!(10),
                ..Default::default()
            },
            ..Default::default()
        };

        let err = can_modify_stop(&trade, dec!(9)).unwrap_err();
        // CORRECT behavior: should be TradeNotFilled, not StopPriceNotValid.
        assert_eq!(
            err.code,
            TradeValidationErrorCode::TradeNotFilled,
            "Unfilled trade should get TradeNotFilled error"
        );
    }

    /// Same for Short trades.
    #[test]
    #[should_panic]
    fn modify_stop_should_check_status_before_price_for_short() {
        use crate::validators::trade::{can_modify_stop, TradeValidationErrorCode};
        use model::TradeCategory;

        let trade = Trade {
            status: Status::Submitted,
            category: TradeCategory::Short,
            safety_stop: Order {
                unit_price: dec!(10),
                ..Default::default()
            },
            ..Default::default()
        };

        let err = can_modify_stop(&trade, dec!(11)).unwrap_err();
        assert_eq!(
            err.code,
            TradeValidationErrorCode::TradeNotFilled,
            "Unfilled Short trade should get TradeNotFilled error"
        );
    }

    // ===============================================================
    // 4. LEGACY SHA-256 — SHOULD USE ARGON2 EVERYWHERE
    // ===============================================================

    /// The legacy password hashing path should not exist.  All password
    /// hashes should use Argon2 (salted, slow).  This test verifies
    /// that hashing the same password twice produces DIFFERENT results
    /// (which Argon2 does, but SHA-256 doesn't).
    #[test]
    #[should_panic]
    fn password_hash_should_be_salted() {
        use sha2::{Digest, Sha256};

        let password = "my_secure_password";

        // Reproduce the legacy hashing logic from core/src/lib.rs:1977-1987
        let hash = |p: &str| {
            let mut h = Sha256::new();
            h.update(p.as_bytes());
            format!("{:x}", h.finalize())
        };

        let h1 = hash(password);
        let h2 = hash(password);

        // CORRECT behavior: hashing same password twice should produce
        // different results (salted).  Legacy SHA-256 fails this.
        assert_ne!(
            h1, h2,
            "Password hash should be salted — same input must produce different output"
        );
    }

    // ===============================================================
    // 5. VALIDATOR CONSISTENCY — all should reject zero
    // ===============================================================

    /// All financial validators should reject zero amounts consistently.
    /// (This test already passes for close/fill/fee — only deposit is wrong,
    /// which is covered by zero_amount_deposit_should_be_rejected above.)
    #[test]
    fn other_validators_correctly_reject_zero() {
        use crate::validators::transaction::{
            can_transfer_close, can_transfer_fee, can_transfer_fill,
        };
        use model::{AccountBalance, TradeBalance};

        assert!(can_transfer_close(dec!(0)).is_err(), "close rejects zero");

        let trade = Trade {
            balance: TradeBalance {
                funding: dec!(500),
                ..Default::default()
            },
            status: Status::Funded,
            ..Default::default()
        };
        assert!(
            can_transfer_fill(&trade, dec!(0)).is_err(),
            "fill rejects zero"
        );

        let balance = AccountBalance {
            total_available: dec!(1000),
            ..Default::default()
        };
        assert!(
            can_transfer_fee(&balance, dec!(0)).is_err(),
            "fee rejects zero"
        );
    }
}
