//! Test-only helpers for service modules.

#[cfg(test)]
macro_rules! impl_database_factory_not_used {
    ($ty:ty, $message:expr) => {
        impl model::DatabaseFactory for $ty {
            fn account_read(&self) -> Box<dyn model::AccountRead> {
                todo!($message)
            }

            fn account_write(&self) -> Box<dyn model::AccountWrite> {
                todo!($message)
            }

            fn account_balance_read(&self) -> Box<dyn model::AccountBalanceRead> {
                todo!($message)
            }

            fn account_balance_write(&self) -> Box<dyn model::AccountBalanceWrite> {
                todo!($message)
            }

            fn order_read(&self) -> Box<dyn model::OrderRead> {
                todo!($message)
            }

            fn order_write(&self) -> Box<dyn model::OrderWrite> {
                todo!($message)
            }

            fn transaction_read(&self) -> Box<dyn model::ReadTransactionDB> {
                todo!($message)
            }

            fn transaction_write(&self) -> Box<dyn model::WriteTransactionDB> {
                todo!($message)
            }

            fn trade_read(&self) -> Box<dyn model::ReadTradeDB> {
                todo!($message)
            }

            fn trade_write(&self) -> Box<dyn model::WriteTradeDB> {
                todo!($message)
            }

            fn trade_balance_write(&self) -> Box<dyn model::database::WriteAccountBalanceDB> {
                todo!($message)
            }

            fn rule_read(&self) -> Box<dyn model::ReadRuleDB> {
                todo!($message)
            }

            fn rule_write(&self) -> Box<dyn model::WriteRuleDB> {
                todo!($message)
            }

            fn trading_vehicle_read(&self) -> Box<dyn model::ReadTradingVehicleDB> {
                todo!($message)
            }

            fn trading_vehicle_write(&self) -> Box<dyn model::WriteTradingVehicleDB> {
                todo!($message)
            }

            fn log_read(&self) -> Box<dyn model::ReadBrokerLogsDB> {
                todo!($message)
            }

            fn log_write(&self) -> Box<dyn model::WriteBrokerLogsDB> {
                todo!($message)
            }

            fn broker_event_read(&self) -> Box<dyn model::ReadBrokerEventsDB> {
                todo!($message)
            }

            fn broker_event_write(&self) -> Box<dyn model::WriteBrokerEventsDB> {
                todo!($message)
            }

            fn execution_read(&self) -> Box<dyn model::ReadExecutionDB> {
                todo!($message)
            }

            fn execution_write(&self) -> Box<dyn model::WriteExecutionDB> {
                todo!($message)
            }

            fn trade_grade_read(&self) -> Box<dyn model::ReadTradeGradeDB> {
                todo!($message)
            }

            fn trade_grade_write(&self) -> Box<dyn model::WriteTradeGradeDB> {
                todo!($message)
            }

            fn level_read(&self) -> Box<dyn model::ReadLevelDB> {
                todo!($message)
            }

            fn level_write(&self) -> Box<dyn model::WriteLevelDB> {
                todo!($message)
            }

            fn distribution_read(&self) -> Box<dyn model::DistributionRead> {
                todo!($message)
            }

            fn distribution_write(&self) -> Box<dyn model::DistributionWrite> {
                todo!($message)
            }

            fn begin_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn std::error::Error>> {
                Ok(())
            }

            fn release_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn std::error::Error>> {
                Ok(())
            }

            fn rollback_to_savepoint(
                &mut self,
                _name: &str,
            ) -> Result<(), Box<dyn std::error::Error>> {
                Ok(())
            }
        }
    };
}

#[cfg(test)]
pub(crate) use impl_database_factory_not_used;
