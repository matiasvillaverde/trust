use crate::services::ProfitDistributionService;
use model::{Account, AccountType, Currency, DatabaseFactory, DistributionRulesNotFound, Trade};
use rust_decimal::Decimal;
use std::error::Error;

/// Service for handling event-driven automatic profit distribution
/// Listens to trade closure events and triggers distribution when profitable
pub struct EventDistributionService<'a> {
    database: &'a mut dyn DatabaseFactory,
}

impl<'a> std::fmt::Debug for EventDistributionService<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventDistributionService")
            .field("database", &"&mut dyn DatabaseFactory")
            .finish()
    }
}

impl<'a> EventDistributionService<'a> {
    /// Creates a new event distribution service
    pub fn new(database: &'a mut dyn DatabaseFactory) -> Self {
        Self { database }
    }

    /// Handles trade closure event and triggers automatic distribution if profitable
    pub fn handle_trade_closed_event(
        &mut self,
        trade: &Trade,
        currency: &Currency,
    ) -> Result<Option<model::DistributionResult>, Box<dyn Error>> {
        // 1. Check if trade was profitable
        let profit = self.calculate_trade_profit(trade)?;
        if profit <= Decimal::ZERO {
            return Ok(None); // No distribution for losses
        }

        let source_account = self.database.account_read().id(trade.account_id)?;
        let rules = match self
            .database
            .distribution_read()
            .for_account(trade.account_id)
        {
            Ok(rules) => rules,
            Err(error) => {
                if error.downcast_ref::<DistributionRulesNotFound>().is_some() {
                    return Ok(None);
                }
                return Err(error);
            }
        };

        // Rules exist, but threshold can still opt-out.
        if profit < rules.minimum_threshold {
            return Ok(None);
        }

        let (earnings_account, tax_account, reinvestment_account) =
            self.find_distribution_accounts(source_account.id)?;
        let mut distribution_service = ProfitDistributionService::new(self.database);

        let result = distribution_service.execute_distribution(
            &source_account,
            &earnings_account,
            &tax_account,
            &reinvestment_account,
            profit,
            &rules,
            currency,
            Some(trade.id),
        )?;

        Ok(Some(result))
    }

    /// Calculate profit from a closed trade
    fn calculate_trade_profit(&self, trade: &Trade) -> Result<Decimal, Box<dyn Error>> {
        // Use the total_performance field which represents profit/loss
        Ok(trade.balance.total_performance)
    }

    fn find_distribution_accounts(
        &mut self,
        source_account_id: uuid::Uuid,
    ) -> Result<(Account, Account, Account), Box<dyn Error>> {
        let child_accounts: Vec<Account> = self
            .database
            .account_read()
            .all()?
            .into_iter()
            .filter(|account| account.parent_account_id == Some(source_account_id))
            .collect();

        let earnings_account = child_accounts
            .iter()
            .find(|acc| acc.account_type == AccountType::Earnings)
            .cloned()
            .ok_or("Earnings account not found")?;
        let tax_account = child_accounts
            .iter()
            .find(|acc| acc.account_type == AccountType::TaxReserve)
            .cloned()
            .ok_or("Tax reserve account not found")?;
        let reinvestment_account = child_accounts
            .iter()
            .find(|acc| acc.account_type == AccountType::Reinvestment)
            .cloned()
            .ok_or("Reinvestment account not found")?;

        Ok((earnings_account, tax_account, reinvestment_account))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use model::{Currency, DatabaseFactory, DistributionRulesNotFound, Status};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    // Mock database factory for testing
    #[derive(Debug)]
    struct MockDatabaseFactory;

    impl DatabaseFactory for MockDatabaseFactory {
        fn account_read(&self) -> Box<dyn model::AccountRead> {
            todo!("Mock not needed for this test")
        }

        fn account_write(&self) -> Box<dyn model::AccountWrite> {
            todo!("Mock not needed for this test")
        }

        fn account_balance_read(&self) -> Box<dyn model::AccountBalanceRead> {
            todo!("Mock not needed for this test")
        }

        fn account_balance_write(&self) -> Box<dyn model::AccountBalanceWrite> {
            todo!("Mock not needed for this test")
        }

        fn order_read(&self) -> Box<dyn model::OrderRead> {
            todo!("Mock not needed for this test")
        }

        fn order_write(&self) -> Box<dyn model::OrderWrite> {
            todo!("Mock not needed for this test")
        }

        fn transaction_read(&self) -> Box<dyn model::ReadTransactionDB> {
            todo!("Mock not needed for this test")
        }

        fn transaction_write(&self) -> Box<dyn model::WriteTransactionDB> {
            todo!("Mock not needed for this test")
        }

        fn trade_read(&self) -> Box<dyn model::ReadTradeDB> {
            todo!("Mock not needed for this test")
        }

        fn trade_write(&self) -> Box<dyn model::WriteTradeDB> {
            todo!("Mock not needed for this test")
        }

        fn trade_balance_write(&self) -> Box<dyn model::database::WriteAccountBalanceDB> {
            todo!("Mock not needed for this test")
        }

        fn rule_read(&self) -> Box<dyn model::ReadRuleDB> {
            todo!("Mock not needed for this test")
        }

        fn rule_write(&self) -> Box<dyn model::WriteRuleDB> {
            todo!("Mock not needed for this test")
        }

        fn trading_vehicle_read(&self) -> Box<dyn model::ReadTradingVehicleDB> {
            todo!("Mock not needed for this test")
        }

        fn trading_vehicle_write(&self) -> Box<dyn model::WriteTradingVehicleDB> {
            todo!("Mock not needed for this test")
        }

        fn log_read(&self) -> Box<dyn model::ReadBrokerLogsDB> {
            todo!("Mock not needed for this test")
        }

        fn log_write(&self) -> Box<dyn model::WriteBrokerLogsDB> {
            todo!("Mock not needed for this test")
        }

        fn distribution_read(&self) -> Box<dyn model::DistributionRead> {
            todo!("Mock not needed for this test")
        }

        fn distribution_write(&self) -> Box<dyn model::DistributionWrite> {
            todo!("Mock not needed for this test")
        }

        fn trade_grade_read(&self) -> Box<dyn model::ReadTradeGradeDB> {
            todo!("Mock not needed for this test")
        }

        fn trade_grade_write(&self) -> Box<dyn model::WriteTradeGradeDB> {
            todo!("Mock not needed for this test")
        }

        fn level_read(&self) -> Box<dyn model::ReadLevelDB> {
            todo!("Mock not needed for this test")
        }

        fn level_write(&self) -> Box<dyn model::WriteLevelDB> {
            todo!("Mock not needed for this test")
        }

        fn begin_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
            Ok(())
        }

        fn release_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
            Ok(())
        }

        fn rollback_to_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
    }

    fn create_test_trade_profitable() -> Trade {
        use model::{Currency, TradeBalance};

        let mut trade = Trade {
            status: Status::ClosedTarget,
            ..Default::default()
        };

        // Create profitable balance
        let now = Utc::now().naive_utc();
        trade.balance = TradeBalance {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: Currency::USD,
            funding: dec!(1000.0),            // Initial investment
            capital_in_market: dec!(0.0),     // No longer in market (closed)
            capital_out_market: dec!(1200.0), // Total capital out
            taxed: dec!(50.0),                // Tax amount
            total_performance: dec!(200.0),   // Profit: 1200 - 1000 = 200
        };

        trade
    }

    fn create_test_trade_loss() -> Trade {
        let mut trade = create_test_trade_profitable();
        trade.balance.capital_out_market = dec!(800.0); // Loss
        trade.balance.total_performance = dec!(-200.0); // Negative profit
        trade.status = Status::ClosedStopLoss;
        trade
    }

    #[test]
    fn test_calculate_trade_profit_profitable() {
        // Given: Event distribution service
        let mut mock_db = MockDatabaseFactory;
        let service = EventDistributionService::new(&mut mock_db);

        // And: A profitable trade
        let trade = create_test_trade_profitable();

        // When: Calculate profit
        let profit = service.calculate_trade_profit(&trade).unwrap();

        // Then: Should return positive profit (1200 - 1000 = 200)
        assert_eq!(profit, dec!(200.0));
    }

    #[test]
    fn test_calculate_trade_profit_loss() {
        // Given: Event distribution service
        let mut mock_db = MockDatabaseFactory;
        let service = EventDistributionService::new(&mut mock_db);

        // And: A losing trade
        let trade = create_test_trade_loss();

        // When: Calculate profit
        let profit = service.calculate_trade_profit(&trade).unwrap();

        // Then: Should return negative profit (800 - 1000 = -200)
        assert_eq!(profit, dec!(-200.0));
    }

    #[test]
    fn test_handle_trade_closed_event_loss_no_distribution() {
        // Given: Event distribution service
        let mut mock_db = MockDatabaseFactory;
        let mut service = EventDistributionService::new(&mut mock_db);

        // And: A losing trade
        let trade = create_test_trade_loss();
        let currency = Currency::USD;

        // When: Handle trade closed event
        let result = service
            .handle_trade_closed_event(&trade, &currency)
            .unwrap();

        // Then: Should return None (no distribution for losses)
        assert!(result.is_none());
    }

    #[test]
    fn test_event_distribution_integration() {
        let mut mock_db = MockDatabaseFactory;
        let service = EventDistributionService::new(&mut mock_db);
        let trade = create_test_trade_profitable();

        // The event service should identify profitable trades deterministically.
        let profit = service.calculate_trade_profit(&trade).unwrap();
        assert!(profit > Decimal::ZERO);
    }

    #[derive(Debug)]
    struct FailingDistributionRead;

    impl model::DistributionRead for FailingDistributionRead {
        fn for_account(
            &mut self,
            _account_id: uuid::Uuid,
        ) -> Result<model::DistributionRules, Box<dyn std::error::Error>> {
            Err("database unavailable".into())
        }

        fn history_for_account(
            &mut self,
            _account_id: uuid::Uuid,
        ) -> Result<Vec<model::DistributionHistory>, Box<dyn std::error::Error>> {
            Ok(Vec::new())
        }
    }

    #[derive(Debug)]
    struct NotFoundDistributionRead {
        account_id: uuid::Uuid,
    }

    impl model::DistributionRead for NotFoundDistributionRead {
        fn for_account(
            &mut self,
            _account_id: uuid::Uuid,
        ) -> Result<model::DistributionRules, Box<dyn std::error::Error>> {
            Err(DistributionRulesNotFound {
                account_id: self.account_id,
            }
            .into())
        }

        fn history_for_account(
            &mut self,
            _account_id: uuid::Uuid,
        ) -> Result<Vec<model::DistributionHistory>, Box<dyn std::error::Error>> {
            Ok(Vec::new())
        }
    }

    #[derive(Debug)]
    struct FixedAccountRead {
        account: Account,
    }

    impl model::AccountRead for FixedAccountRead {
        fn for_name(&mut self, _name: &str) -> Result<Account, Box<dyn std::error::Error>> {
            Ok(self.account.clone())
        }

        fn id(&mut self, _id: uuid::Uuid) -> Result<Account, Box<dyn std::error::Error>> {
            Ok(self.account.clone())
        }

        fn all(&mut self) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
            Ok(vec![self.account.clone()])
        }
    }

    #[derive(Debug)]
    struct ErrorPropagationDatabase {
        account: Account,
        return_not_found: bool,
    }

    impl DatabaseFactory for ErrorPropagationDatabase {
        fn account_read(&self) -> Box<dyn model::AccountRead> {
            Box::new(FixedAccountRead {
                account: self.account.clone(),
            })
        }
        fn account_write(&self) -> Box<dyn model::AccountWrite> {
            todo!("not used")
        }
        fn account_balance_read(&self) -> Box<dyn model::AccountBalanceRead> {
            todo!("not used")
        }
        fn account_balance_write(&self) -> Box<dyn model::AccountBalanceWrite> {
            todo!("not used")
        }
        fn order_read(&self) -> Box<dyn model::OrderRead> {
            todo!("not used")
        }
        fn order_write(&self) -> Box<dyn model::OrderWrite> {
            todo!("not used")
        }
        fn transaction_read(&self) -> Box<dyn model::ReadTransactionDB> {
            todo!("not used")
        }
        fn transaction_write(&self) -> Box<dyn model::WriteTransactionDB> {
            todo!("not used")
        }
        fn trade_read(&self) -> Box<dyn model::ReadTradeDB> {
            todo!("not used")
        }
        fn trade_write(&self) -> Box<dyn model::WriteTradeDB> {
            todo!("not used")
        }
        fn trade_balance_write(&self) -> Box<dyn model::database::WriteAccountBalanceDB> {
            todo!("not used")
        }
        fn rule_read(&self) -> Box<dyn model::ReadRuleDB> {
            todo!("not used")
        }
        fn rule_write(&self) -> Box<dyn model::WriteRuleDB> {
            todo!("not used")
        }
        fn trading_vehicle_read(&self) -> Box<dyn model::ReadTradingVehicleDB> {
            todo!("not used")
        }
        fn trading_vehicle_write(&self) -> Box<dyn model::WriteTradingVehicleDB> {
            todo!("not used")
        }
        fn log_read(&self) -> Box<dyn model::ReadBrokerLogsDB> {
            todo!("not used")
        }
        fn log_write(&self) -> Box<dyn model::WriteBrokerLogsDB> {
            todo!("not used")
        }
        fn distribution_read(&self) -> Box<dyn model::DistributionRead> {
            if self.return_not_found {
                Box::new(NotFoundDistributionRead {
                    account_id: self.account.id,
                })
            } else {
                Box::new(FailingDistributionRead)
            }
        }
        fn distribution_write(&self) -> Box<dyn model::DistributionWrite> {
            todo!("not used")
        }

        fn trade_grade_read(&self) -> Box<dyn model::ReadTradeGradeDB> {
            todo!("not used")
        }

        fn trade_grade_write(&self) -> Box<dyn model::WriteTradeGradeDB> {
            todo!("not used")
        }

        fn level_read(&self) -> Box<dyn model::ReadLevelDB> {
            todo!("not used")
        }

        fn level_write(&self) -> Box<dyn model::WriteLevelDB> {
            todo!("not used")
        }

        fn begin_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
            Ok(())
        }

        fn release_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
            Ok(())
        }

        fn rollback_to_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
    }

    #[test]
    fn test_handle_trade_closed_event_propagates_distribution_read_errors() {
        let trade = create_test_trade_profitable();
        let source_account = Account {
            id: trade.account_id,
            ..Account::default()
        };
        let mut db = ErrorPropagationDatabase {
            account: source_account,
            return_not_found: false,
        };
        let mut service = EventDistributionService::new(&mut db);

        let result = service.handle_trade_closed_event(&trade, &Currency::USD);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("database unavailable"));
    }

    #[test]
    fn test_handle_trade_closed_event_treats_rules_not_found_as_none() {
        let trade = create_test_trade_profitable();
        let source_account = Account {
            id: trade.account_id,
            ..Account::default()
        };
        let mut db = ErrorPropagationDatabase {
            account: source_account,
            return_not_found: true,
        };
        let mut service = EventDistributionService::new(&mut db);

        let result = service.handle_trade_closed_event(&trade, &Currency::USD);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
