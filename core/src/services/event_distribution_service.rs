use crate::services::ProfitDistributionService;
use model::{Account, AccountType, Currency, DatabaseFactory, Trade};
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
            Err(_) => return Ok(None),
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
    use model::{Currency, DatabaseFactory, Status};
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
}
