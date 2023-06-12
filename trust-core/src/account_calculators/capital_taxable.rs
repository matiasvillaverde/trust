use rust_decimal::Decimal;
use trust_model::{Currency, ReadTransactionDB, TransactionCategory};
use uuid::Uuid;

pub struct AccountCapitalTaxable;

impl AccountCapitalTaxable {
    pub fn calculate(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions = database.read_all_account_transactions_taxes(account_id, currency)?;

        // Sum all transactions
        let total: Decimal = transactions
            .iter()
            .map(|transaction| match transaction.category {
                TransactionCategory::PaymentTax(_) => transaction.price,
                TransactionCategory::WithdrawalTax => -transaction.price,
                default => panic!(
                    "capital_taxable: does not know how to calculate transaction with category: {}",
                    default
                ),
            })
            .sum();

        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::read_transaction_db_mocks::MockDatabase;
    use rust_decimal_macros::dec;

    #[test]
    fn test_capital_taxable_with_empty_transactions() {
        let mut database = MockDatabase::new();

        let result =
            AccountCapitalTaxable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_capital_taxable_with_one_transaction() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(100));

        let result =
            AccountCapitalTaxable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(100));
    }

    #[test]
    fn test_capital_taxable_many_transactions() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(100.7));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(32492));
        database.set_transaction(
            TransactionCategory::PaymentTax(Uuid::new_v4()),
            dec!(383.322),
        );

        let result =
            AccountCapitalTaxable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(32976.022));
    }

    #[test]
    fn test_capital_taxable_many_transactions_including_withdrawals() {
        let mut database = MockDatabase::new();

        // One deposit and one withdrawal transaction in the database
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(7.7));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(934));
        database.set_transaction(TransactionCategory::WithdrawalTax, dec!(38.322));

        let result =
            AccountCapitalTaxable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(903.378));
    }

    #[test]
    #[should_panic(
        expected = "capital_taxable: does not know how to calculate transaction with category: deposit"
    )]
    fn test_capital_taxable_with_unknown_category() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));

        AccountCapitalTaxable::calculate(Uuid::new_v4(), &Currency::USD, &mut database).unwrap();
    }
}
