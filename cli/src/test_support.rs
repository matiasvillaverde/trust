#![allow(clippy::panic, clippy::too_many_lines)]

use chrono::{Duration, Utc};
use model::{
    Account, AccountBalance, AccountBalanceRead, AccountBalanceWrite, AccountRead, AccountWrite,
    AdvisoryRead, AdvisoryThresholds, AdvisoryWrite, BrokerLog, Currency, DatabaseFactory,
    DistributionHistory, DistributionRead, DistributionRules, DistributionWrite, Execution, Grade,
    Level, LevelAdjustmentRules, LevelChange, Order, OrderAction, OrderCategory, OrderRead,
    OrderWrite, ReadBrokerLogsDB, ReadExecutionDB, ReadLevelDB, ReadMistakeDB, ReadRuleDB,
    ReadSessionPlanDB, ReadTradeDB, ReadTradeEventDB, ReadTradeGradeDB, ReadTradingVehicleDB,
    ReadTransactionDB, Rule, RuleLevel, RuleName, Status, Trade, TradeBalance, TradeEvent,
    TradeGrade, TradingVehicle, TradingVehicleCategory, Transaction, TransactionCategory,
    WriteBrokerLogsDB, WriteExecutionDB, WriteLevelDB, WriteMistakeDB, WriteRuleDB,
    WriteSessionPlanDB, WriteTradeDB, WriteTradeEventDB, WriteTradeGradeDB, WriteTradingVehicleDB,
    WriteTransactionDB,
};
use rust_decimal::Decimal;
use std::cell::Cell;
use std::error::Error;
use uuid::Uuid;

pub(crate) struct ReadFailureFactory {
    read: ReadFailure,
    account_read_calls: Cell<usize>,
    level_read_calls: Cell<usize>,
}

enum ReadFailure {
    Accounts,
    Advisory,
    Balances,
    CapitalAtRiskOverflow,
    ClosedTradeNoGradeThenReadError,
    DrawdownEquityOverflow,
    DrawdownMetricsOverflow,
    ExistingEmptyTradeGrade,
    ExistingTradeGradeThenTradeFailure,
    LevelFailsAfterQuantity,
    Levels,
    LevelWrite,
    OpenPositionsThenAccountsFail,
    OrderWriteAfterReads,
    SubmittedTradeMissingAccount,
    TradesAfterAccount,
    Trades,
    TradeGrades,
    Transactions,
    Rules,
    TradingVehicles,
    TradingVehiclesAfterAccount,
    TradingVehicleWrite,
}

impl ReadFailureFactory {
    fn new(read: ReadFailure) -> Self {
        Self {
            read,
            account_read_calls: Cell::new(0),
            level_read_calls: Cell::new(0),
        }
    }

    pub(crate) fn accounts() -> Self {
        Self::new(ReadFailure::Accounts)
    }

    pub(crate) fn advisory() -> Self {
        Self::new(ReadFailure::Advisory)
    }

    pub(crate) fn balances() -> Self {
        Self::new(ReadFailure::Balances)
    }

    pub(crate) fn capital_at_risk_overflow() -> Self {
        Self::new(ReadFailure::CapitalAtRiskOverflow)
    }

    pub(crate) fn closed_trade_without_grade_then_trade_read_failure() -> Self {
        Self::new(ReadFailure::ClosedTradeNoGradeThenReadError)
    }

    pub(crate) fn drawdown_equity_overflow() -> Self {
        Self::new(ReadFailure::DrawdownEquityOverflow)
    }

    pub(crate) fn drawdown_metrics_overflow() -> Self {
        Self::new(ReadFailure::DrawdownMetricsOverflow)
    }

    pub(crate) fn existing_empty_trade_grade() -> Self {
        Self::new(ReadFailure::ExistingEmptyTradeGrade)
    }

    pub(crate) fn existing_trade_grade_then_trade_failure() -> Self {
        Self::new(ReadFailure::ExistingTradeGradeThenTradeFailure)
    }

    pub(crate) fn level_fails_after_quantity() -> Self {
        Self::new(ReadFailure::LevelFailsAfterQuantity)
    }

    pub(crate) fn levels() -> Self {
        Self::new(ReadFailure::Levels)
    }

    pub(crate) fn level_write_after_read() -> Self {
        Self::new(ReadFailure::LevelWrite)
    }

    pub(crate) fn open_positions_then_accounts_fail() -> Self {
        Self::new(ReadFailure::OpenPositionsThenAccountsFail)
    }

    pub(crate) fn order_write_after_reads() -> Self {
        Self::new(ReadFailure::OrderWriteAfterReads)
    }

    pub(crate) fn submitted_trade_missing_account() -> Self {
        Self::new(ReadFailure::SubmittedTradeMissingAccount)
    }

    pub(crate) fn trades_after_account() -> Self {
        Self::new(ReadFailure::TradesAfterAccount)
    }

    pub(crate) fn trades() -> Self {
        Self::new(ReadFailure::Trades)
    }

    pub(crate) fn trade_grades() -> Self {
        Self::new(ReadFailure::TradeGrades)
    }

    pub(crate) fn transactions() -> Self {
        Self::new(ReadFailure::Transactions)
    }

    pub(crate) fn rules() -> Self {
        Self::new(ReadFailure::Rules)
    }

    pub(crate) fn trading_vehicles() -> Self {
        Self::new(ReadFailure::TradingVehicles)
    }

    pub(crate) fn trading_vehicles_after_account() -> Self {
        Self::new(ReadFailure::TradingVehiclesAfterAccount)
    }

    pub(crate) fn trading_vehicle_write() -> Self {
        Self::new(ReadFailure::TradingVehicleWrite)
    }
}

impl DatabaseFactory for ReadFailureFactory {
    fn account_read(&self) -> Box<dyn AccountRead> {
        match self.read {
            ReadFailure::Accounts => Box::new(FailingAccountRead),
            ReadFailure::OpenPositionsThenAccountsFail => {
                let calls = self.account_read_calls.get();
                self.account_read_calls.set(calls.saturating_add(1));
                if calls == 0 {
                    Box::new(SuccessfulAccountRead)
                } else {
                    Box::new(FailingAccountRead)
                }
            }
            ReadFailure::OrderWriteAfterReads
            | ReadFailure::SubmittedTradeMissingAccount
            | ReadFailure::TradesAfterAccount
            | ReadFailure::TradingVehiclesAfterAccount => Box::new(SuccessfulAccountRead),
            _ => panic!("account_read should not be called"),
        }
    }

    fn account_write(&self) -> Box<dyn AccountWrite> {
        panic!("account_write should not be called")
    }

    fn account_balance_read(&self) -> Box<dyn AccountBalanceRead> {
        match self.read {
            ReadFailure::Balances => Box::new(FailingAccountBalanceRead),
            _ => panic!("account_balance_read should not be called"),
        }
    }

    fn account_balance_write(&self) -> Box<dyn AccountBalanceWrite> {
        panic!("account_balance_write should not be called")
    }

    fn order_read(&self) -> Box<dyn OrderRead> {
        panic!("order_read should not be called")
    }

    fn order_write(&self) -> Box<dyn OrderWrite> {
        match self.read {
            ReadFailure::OrderWriteAfterReads => Box::new(FailingOrderWrite),
            _ => panic!("order_write should not be called"),
        }
    }

    fn transaction_read(&self) -> Box<dyn ReadTransactionDB> {
        match self.read {
            ReadFailure::CapitalAtRiskOverflow => Box::new(CapitalAtRiskOverflowTransactionRead),
            ReadFailure::DrawdownEquityOverflow => Box::new(DrawdownEquityOverflowRead),
            ReadFailure::DrawdownMetricsOverflow => Box::new(DrawdownMetricsOverflowRead),
            ReadFailure::LevelFailsAfterQuantity => Box::new(PositiveTransactionRead),
            ReadFailure::Transactions => Box::new(FailingAccountRead),
            _ => panic!("transaction_read should not be called"),
        }
    }

    fn transaction_write(&self) -> Box<dyn WriteTransactionDB> {
        panic!("transaction_write should not be called")
    }

    fn trade_read(&self) -> Box<dyn ReadTradeDB> {
        match self.read {
            ReadFailure::CapitalAtRiskOverflow => Box::new(CapitalAtRiskOverflowTradeRead),
            ReadFailure::Trades
            | ReadFailure::TradesAfterAccount
            | ReadFailure::ExistingTradeGradeThenTradeFailure => Box::new(FailingTradeRead),
            ReadFailure::ClosedTradeNoGradeThenReadError => Box::new(ClosedTradeThenReadFailure),
            ReadFailure::OpenPositionsThenAccountsFail => Box::new(EmptyTradeRead),
            ReadFailure::SubmittedTradeMissingAccount => Box::new(SubmittedTradeRead),
            _ => panic!("trade_read should not be called"),
        }
    }

    fn trade_write(&self) -> Box<dyn WriteTradeDB> {
        panic!("trade_write should not be called")
    }

    fn trade_balance_write(&self) -> Box<dyn model::database::WriteAccountBalanceDB> {
        panic!("trade_balance_write should not be called")
    }

    fn rule_read(&self) -> Box<dyn ReadRuleDB> {
        match self.read {
            ReadFailure::LevelFailsAfterQuantity => Box::new(EmptyRuleRead),
            ReadFailure::Rules => Box::new(FailingRuleRead),
            _ => panic!("rule_read should not be called"),
        }
    }

    fn rule_write(&self) -> Box<dyn WriteRuleDB> {
        panic!("rule_write should not be called")
    }

    fn trading_vehicle_read(&self) -> Box<dyn ReadTradingVehicleDB> {
        match self.read {
            ReadFailure::TradingVehicles | ReadFailure::TradingVehiclesAfterAccount => {
                Box::new(FailingTradingVehicleRead)
            }
            ReadFailure::OrderWriteAfterReads => Box::new(SuccessfulTradingVehicleRead),
            _ => panic!("trading_vehicle_read should not be called"),
        }
    }

    fn trading_vehicle_write(&self) -> Box<dyn WriteTradingVehicleDB> {
        match self.read {
            ReadFailure::TradingVehicleWrite => Box::new(FailingAccountRead),
            _ => panic!("trading_vehicle_write should not be called"),
        }
    }

    fn log_read(&self) -> Box<dyn ReadBrokerLogsDB> {
        panic!("log_read should not be called")
    }

    fn log_write(&self) -> Box<dyn WriteBrokerLogsDB> {
        panic!("log_write should not be called")
    }

    fn execution_read(&self) -> Box<dyn ReadExecutionDB> {
        panic!("execution_read should not be called")
    }

    fn execution_write(&self) -> Box<dyn WriteExecutionDB> {
        panic!("execution_write should not be called")
    }

    fn mistake_read(&self) -> Box<dyn ReadMistakeDB> {
        panic!("mistake_read should not be called")
    }

    fn mistake_write(&self) -> Box<dyn WriteMistakeDB> {
        panic!("mistake_write should not be called")
    }

    fn session_plan_read(&self) -> Box<dyn ReadSessionPlanDB> {
        panic!("session_plan_read should not be called")
    }

    fn session_plan_write(&self) -> Box<dyn WriteSessionPlanDB> {
        panic!("session_plan_write should not be called")
    }

    fn trade_event_read(&self) -> Box<dyn ReadTradeEventDB> {
        panic!("trade_event_read should not be called")
    }

    fn trade_event_write(&self) -> Box<dyn WriteTradeEventDB> {
        panic!("trade_event_write should not be called")
    }

    fn trade_grade_read(&self) -> Box<dyn ReadTradeGradeDB> {
        match self.read {
            ReadFailure::ExistingEmptyTradeGrade
            | ReadFailure::ExistingTradeGradeThenTradeFailure => Box::new(SuccessfulTradeGradeRead),
            ReadFailure::ClosedTradeNoGradeThenReadError => Box::new(MissingTradeGradeRead),
            ReadFailure::TradeGrades => Box::new(FailingTradeGradeRead),
            _ => panic!("trade_grade_read should not be called"),
        }
    }

    fn trade_grade_write(&self) -> Box<dyn WriteTradeGradeDB> {
        panic!("trade_grade_write should not be called")
    }

    fn level_read(&self) -> Box<dyn ReadLevelDB> {
        match self.read {
            ReadFailure::LevelFailsAfterQuantity => {
                let calls = self.level_read_calls.get();
                self.level_read_calls.set(calls.saturating_add(1));
                if calls == 0 {
                    Box::new(SuccessfulLevelRead)
                } else {
                    Box::new(FailingAccountRead)
                }
            }
            ReadFailure::Levels => Box::new(FailingAccountRead),
            ReadFailure::LevelWrite => Box::new(SuccessfulLevelRead),
            _ => panic!("level_read should not be called"),
        }
    }

    fn level_write(&self) -> Box<dyn WriteLevelDB> {
        match self.read {
            ReadFailure::LevelWrite => Box::new(FailingAccountRead),
            _ => panic!("level_write should not be called"),
        }
    }

    fn begin_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
        panic!("begin_savepoint should not be called")
    }

    fn release_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
        panic!("release_savepoint should not be called")
    }

    fn rollback_to_savepoint(&mut self, _name: &str) -> Result<(), Box<dyn Error>> {
        panic!("rollback_to_savepoint should not be called")
    }

    fn distribution_read(&self) -> Box<dyn DistributionRead> {
        panic!("distribution_read should not be called")
    }

    fn distribution_write(&self) -> Box<dyn DistributionWrite> {
        panic!("distribution_write should not be called")
    }

    fn advisory_read(&self) -> Box<dyn AdvisoryRead> {
        match self.read {
            ReadFailure::Advisory => Box::new(FailingAdvisoryRead),
            _ => panic!("advisory_read should not be called"),
        }
    }

    fn advisory_write(&self) -> Box<dyn AdvisoryWrite> {
        panic!("advisory_write should not be called")
    }
}

struct FailingAccountRead;

impl AccountRead for FailingAccountRead {
    fn for_name(&mut self, _name: &str) -> Result<Account, Box<dyn Error>> {
        Err("account read failed".into())
    }

    fn id(&mut self, _id: Uuid) -> Result<Account, Box<dyn Error>> {
        Err("account read failed".into())
    }

    fn all(&mut self) -> Result<Vec<Account>, Box<dyn Error>> {
        Err("account read failed".into())
    }
}

struct SuccessfulAccountRead;

impl AccountRead for SuccessfulAccountRead {
    fn for_name(&mut self, name: &str) -> Result<Account, Box<dyn Error>> {
        Ok(Account {
            name: name.to_string(),
            ..Account::default()
        })
    }

    fn id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>> {
        Ok(Account {
            id,
            name: "readable-account".to_string(),
            ..Account::default()
        })
    }

    fn all(&mut self) -> Result<Vec<Account>, Box<dyn Error>> {
        Ok(vec![Account {
            name: "readable-account".to_string(),
            ..Account::default()
        }])
    }
}

struct FailingAccountBalanceRead;

impl AccountBalanceRead for FailingAccountBalanceRead {
    fn for_account(&mut self, _account_id: Uuid) -> Result<Vec<AccountBalance>, Box<dyn Error>> {
        Err("balance read failed".into())
    }

    fn for_currency(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn Error>> {
        Err("balance read failed".into())
    }
}

struct FailingTradeRead;

impl ReadTradeDB for FailingTradeRead {
    fn all_open_trades_for_currency(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trades_with_status(
        &mut self,
        _account_id: Uuid,
        _status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trade(&mut self, _id: Uuid) -> Result<Trade, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trade_status(&mut self, _id: Uuid) -> Result<Status, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trade_balance(&mut self, _balance_id: Uuid) -> Result<TradeBalance, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_recent_closed_trade_performances(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<model::ClosedTradePerformance>, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_recent_closed_trade_performance_points(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<(chrono::NaiveDateTime, Decimal)>, Box<dyn Error>> {
        Err("trade read failed".into())
    }
}

struct EmptyTradeRead;

impl ReadTradeDB for EmptyTradeRead {
    fn all_open_trades_for_currency(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_trades_with_status(
        &mut self,
        _account_id: Uuid,
        _status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_trade(&mut self, _id: Uuid) -> Result<Trade, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trade_status(&mut self, _id: Uuid) -> Result<Status, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trade_balance(&mut self, _balance_id: Uuid) -> Result<TradeBalance, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_recent_closed_trade_performances(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<model::ClosedTradePerformance>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_recent_closed_trade_performance_points(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<(chrono::NaiveDateTime, Decimal)>, Box<dyn Error>> {
        Ok(vec![])
    }
}

struct CapitalAtRiskOverflowTradeRead;

impl ReadTradeDB for CapitalAtRiskOverflowTradeRead {
    fn all_open_trades_for_currency(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_trades_with_status(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        if status == Status::Filled {
            return Ok(vec![
                Trade {
                    id: Uuid::from_u128(1),
                    account_id,
                    status,
                    ..Trade::default()
                },
                Trade {
                    id: Uuid::from_u128(2),
                    account_id,
                    status,
                    ..Trade::default()
                },
            ]);
        }
        Ok(vec![])
    }

    fn read_trade(&mut self, _id: Uuid) -> Result<Trade, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trade_status(&mut self, _id: Uuid) -> Result<Status, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trade_balance(&mut self, _balance_id: Uuid) -> Result<TradeBalance, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_recent_closed_trade_performances(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<model::ClosedTradePerformance>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_recent_closed_trade_performance_points(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<(chrono::NaiveDateTime, Decimal)>, Box<dyn Error>> {
        Ok(vec![])
    }
}

fn transaction_fixture(
    category: TransactionCategory,
    amount: Decimal,
    offset_seconds: i64,
) -> Transaction {
    let now = Utc::now().naive_utc();
    let created_at = now
        .checked_add_signed(Duration::seconds(offset_seconds))
        .unwrap_or(now);
    Transaction {
        id: Uuid::new_v4(),
        category,
        currency: Currency::USD,
        amount,
        account_id: Uuid::new_v4(),
        created_at,
        updated_at: created_at,
        deleted_at: None,
    }
}

struct PositiveTransactionRead;

impl ReadTransactionDB for PositiveTransactionRead {
    fn all_account_transactions_excluding_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![transaction_fixture(
            TransactionCategory::Deposit,
            Decimal::from(10_000),
            0,
        )])
    }

    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_all_account_transactions_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_funding_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_taxes_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_transactions(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }
}

struct CapitalAtRiskOverflowTransactionRead;

impl ReadTransactionDB for CapitalAtRiskOverflowTransactionRead {
    fn all_account_transactions_excluding_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_all_account_transactions_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_funding_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![transaction_fixture(
            TransactionCategory::FundTrade(trade_id),
            Decimal::MAX,
            0,
        )])
    }

    fn all_trade_taxes_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_transactions(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }
}

struct DrawdownEquityOverflowRead;

impl ReadTransactionDB for DrawdownEquityOverflowRead {
    fn all_account_transactions_excluding_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![
            transaction_fixture(TransactionCategory::Deposit, Decimal::MAX, 0),
            transaction_fixture(TransactionCategory::Deposit, Decimal::MAX, 1),
        ])
    }

    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_all_account_transactions_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_funding_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_taxes_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_transactions(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }
}

struct DrawdownMetricsOverflowRead;

impl ReadTransactionDB for DrawdownMetricsOverflowRead {
    fn all_account_transactions_excluding_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![
            transaction_fixture(TransactionCategory::Deposit, Decimal::ONE, 0),
            transaction_fixture(TransactionCategory::Withdrawal, Decimal::MAX, 1),
        ])
    }

    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_all_account_transactions_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_funding_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_trade_taxes_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn all_transactions(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Ok(vec![])
    }
}

struct SubmittedTradeRead;

impl ReadTradeDB for SubmittedTradeRead {
    fn all_open_trades_for_currency(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_trades_with_status(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        if status == Status::Submitted {
            return Ok(vec![Trade {
                id: Uuid::nil(),
                account_id,
                status,
                updated_at: Utc::now().naive_utc(),
                ..Trade::default()
            }]);
        }
        Ok(vec![])
    }

    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>> {
        Ok(Trade {
            id,
            status: Status::Submitted,
            updated_at: Utc::now().naive_utc(),
            ..Trade::default()
        })
    }

    fn read_trade_status(&mut self, _id: Uuid) -> Result<Status, Box<dyn Error>> {
        Ok(Status::Submitted)
    }

    fn read_trade_balance(&mut self, _balance_id: Uuid) -> Result<TradeBalance, Box<dyn Error>> {
        Ok(TradeBalance::default())
    }

    fn read_recent_closed_trade_performances(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<model::ClosedTradePerformance>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_recent_closed_trade_performance_points(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<(chrono::NaiveDateTime, Decimal)>, Box<dyn Error>> {
        Ok(vec![])
    }
}

struct ClosedTradeThenReadFailure;

impl ReadTradeDB for ClosedTradeThenReadFailure {
    fn all_open_trades_for_currency(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn read_trades_with_status(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        if status == Status::ClosedTarget {
            return Ok(vec![Trade {
                id: Uuid::new_v4(),
                account_id,
                status,
                updated_at: Utc::now().naive_utc(),
                ..Trade::default()
            }]);
        }
        Ok(vec![])
    }

    fn read_trade(&mut self, _id: Uuid) -> Result<Trade, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trade_status(&mut self, _id: Uuid) -> Result<Status, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_trade_balance(&mut self, _balance_id: Uuid) -> Result<TradeBalance, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_recent_closed_trade_performances(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<model::ClosedTradePerformance>, Box<dyn Error>> {
        Err("trade read failed".into())
    }

    fn read_recent_closed_trade_performance_points(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
        _cutoff: chrono::NaiveDateTime,
    ) -> Result<Vec<(chrono::NaiveDateTime, Decimal)>, Box<dyn Error>> {
        Err("trade read failed".into())
    }
}

struct FailingRuleRead;

impl ReadRuleDB for FailingRuleRead {
    fn read_all_rules(&mut self, _account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>> {
        Err("rule read failed".into())
    }

    fn rule_for_account(
        &mut self,
        _account_id: Uuid,
        _name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>> {
        Err("rule read failed".into())
    }
}

struct EmptyRuleRead;

impl ReadRuleDB for EmptyRuleRead {
    fn read_all_rules(&mut self, _account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn rule_for_account(
        &mut self,
        _account_id: Uuid,
        _name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>> {
        Err("rule not found".into())
    }
}

struct FailingTradingVehicleRead;

impl ReadTradingVehicleDB for FailingTradingVehicleRead {
    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        Err("trading vehicle read failed".into())
    }

    fn read_trading_vehicle(&mut self, _id: Uuid) -> Result<TradingVehicle, Box<dyn Error>> {
        Err("trading vehicle read failed".into())
    }
}

struct SuccessfulTradingVehicleRead;

impl ReadTradingVehicleDB for SuccessfulTradingVehicleRead {
    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        Ok(vec![TradingVehicle {
            symbol: "AAPL".to_string(),
            category: TradingVehicleCategory::Stock,
            broker: "alpaca".to_string(),
            ..TradingVehicle::default()
        }])
    }

    fn read_trading_vehicle(&mut self, id: Uuid) -> Result<TradingVehicle, Box<dyn Error>> {
        Ok(TradingVehicle {
            id,
            symbol: "AAPL".to_string(),
            category: TradingVehicleCategory::Stock,
            broker: "alpaca".to_string(),
            ..TradingVehicle::default()
        })
    }
}

struct FailingOrderWrite;

impl OrderWrite for FailingOrderWrite {
    fn create(
        &mut self,
        _trading_vehicle: &TradingVehicle,
        _quantity: Decimal,
        _price: Decimal,
        _currency: &Currency,
        _action: &OrderAction,
        _category: &OrderCategory,
    ) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn submit_of(
        &mut self,
        _order: &Order,
        _broker_order_id: String,
    ) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn filling_of(&mut self, _order: &Order) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn closing_of(&mut self, _order: &Order) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn update(&mut self, _order: &Order) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn update_price(
        &mut self,
        _order: &Order,
        _price: Decimal,
        _broker_id: String,
    ) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }
}

struct FailingAdvisoryRead;

impl AdvisoryRead for FailingAdvisoryRead {
    fn advisory_thresholds_for_account(
        &mut self,
        _account_id: Uuid,
    ) -> Result<Option<AdvisoryThresholds>, Box<dyn Error>> {
        Err("advisory read failed".into())
    }
}

struct FailingTradeGradeRead;

impl ReadTradeGradeDB for FailingTradeGradeRead {
    fn read_latest_for_trade(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Option<TradeGrade>, Box<dyn Error>> {
        Err("trade grade read failed".into())
    }

    fn read_for_account_days(
        &mut self,
        _account_id: Uuid,
        _days: u32,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>> {
        Err("trade grade read failed".into())
    }
}

struct SuccessfulTradeGradeRead;

impl ReadTradeGradeDB for SuccessfulTradeGradeRead {
    fn read_latest_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Option<TradeGrade>, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        Ok(Some(TradeGrade {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id,
            overall_score: 100,
            overall_grade: Grade::APlus,
            process_score: 100,
            risk_score: 100,
            execution_score: 100,
            documentation_score: 100,
            recommendations: Vec::new(),
            graded_at: now,
            process_weight_permille: 400,
            risk_weight_permille: 300,
            execution_weight_permille: 200,
            documentation_weight_permille: 100,
        }))
    }

    fn read_for_account_days(
        &mut self,
        _account_id: Uuid,
        _days: u32,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>> {
        Ok(vec![])
    }
}

struct MissingTradeGradeRead;

impl ReadTradeGradeDB for MissingTradeGradeRead {
    fn read_latest_for_trade(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Option<TradeGrade>, Box<dyn Error>> {
        Ok(None)
    }

    fn read_for_account_days(
        &mut self,
        _account_id: Uuid,
        _days: u32,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>> {
        Ok(vec![])
    }
}

struct SuccessfulLevelRead;

impl ReadLevelDB for SuccessfulLevelRead {
    fn level_for_account(&mut self, account_id: Uuid) -> Result<Level, Box<dyn Error>> {
        Ok(Level::default_for_account(account_id))
    }

    fn level_changes_for_account(
        &mut self,
        _account_id: Uuid,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn recent_level_changes(
        &mut self,
        _account_id: Uuid,
        _days: u32,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        Ok(vec![])
    }

    fn level_adjustment_rules_for_account(
        &mut self,
        _account_id: Uuid,
    ) -> Result<LevelAdjustmentRules, Box<dyn Error>> {
        Ok(LevelAdjustmentRules::default())
    }
}

impl AdvisoryWrite for FailingAccountRead {
    fn upsert_advisory_thresholds(
        &mut self,
        _account_id: Uuid,
        _sector_limit_pct: Decimal,
        _asset_class_limit_pct: Decimal,
        _single_position_limit_pct: Decimal,
    ) -> Result<(), Box<dyn Error>> {
        Err("advisory write failed".into())
    }
}

impl AccountWrite for FailingAccountRead {
    fn create(
        &mut self,
        _name: &str,
        _description: &str,
        _environment: model::Environment,
        _taxes_percentage: Decimal,
        _earnings_percentage: Decimal,
    ) -> Result<Account, Box<dyn Error>> {
        Err("account write failed".into())
    }
}

impl AccountBalanceWrite for FailingAccountRead {
    fn create(
        &mut self,
        _account: &Account,
        _currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn Error>> {
        Err("balance write failed".into())
    }

    fn update(
        &mut self,
        _balance: &AccountBalance,
        _total_balance: Decimal,
        _in_trade: Decimal,
        _available: Decimal,
        _taxed: Decimal,
    ) -> Result<AccountBalance, Box<dyn Error>> {
        Err("balance write failed".into())
    }
}

impl OrderRead for FailingAccountRead {
    fn for_id(&mut self, _id: Uuid) -> Result<Order, Box<dyn Error>> {
        Err("order read failed".into())
    }
}

impl OrderWrite for FailingAccountRead {
    fn create(
        &mut self,
        _trading_vehicle: &TradingVehicle,
        _quantity: Decimal,
        _price: Decimal,
        _currency: &Currency,
        _action: &OrderAction,
        _category: &OrderCategory,
    ) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn submit_of(
        &mut self,
        _order: &Order,
        _broker_order_id: String,
    ) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn filling_of(&mut self, _order: &Order) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn closing_of(&mut self, _order: &Order) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn update(&mut self, _order: &Order) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }

    fn update_price(
        &mut self,
        _order: &Order,
        _price: Decimal,
        _broker_id: String,
    ) -> Result<Order, Box<dyn Error>> {
        Err("order write failed".into())
    }
}

impl WriteTransactionDB for FailingAccountRead {
    fn create_transaction_by_account_id(
        &mut self,
        _account_id: Uuid,
        _amount: Decimal,
        _currency: &Currency,
        _category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        Err("transaction write failed".into())
    }
}

impl ReadTransactionDB for FailingAccountRead {
    fn all_account_transactions_excluding_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Err("transaction read failed".into())
    }

    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Err("transaction read failed".into())
    }

    fn read_all_account_transactions_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Err("transaction read failed".into())
    }

    fn all_trade_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Err("transaction read failed".into())
    }

    fn all_trade_funding_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Err("transaction read failed".into())
    }

    fn all_trade_taxes_transactions(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Err("transaction read failed".into())
    }

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Err("transaction read failed".into())
    }

    fn all_transactions(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        Err("transaction read failed".into())
    }
}

impl model::database::WriteAccountBalanceDB for FailingAccountRead {
    fn update_trade_balance(
        &mut self,
        _trade: &Trade,
        _funding: Decimal,
        _capital_in_market: Decimal,
        _capital_out_market: Decimal,
        _taxed: Decimal,
        _total_performance: Decimal,
    ) -> Result<TradeBalance, Box<dyn Error>> {
        Err("trade balance write failed".into())
    }
}

impl WriteTradeDB for FailingAccountRead {
    fn create_trade(
        &mut self,
        _draft: model::DraftTrade,
        _stop: &Order,
        _entry: &Order,
        _target: &Order,
    ) -> Result<Trade, Box<dyn Error>> {
        Err("trade write failed".into())
    }

    fn update_trade_status(
        &mut self,
        _status: Status,
        _trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>> {
        Err("trade write failed".into())
    }
}

impl WriteRuleDB for FailingAccountRead {
    fn create_rule(
        &mut self,
        _account: &Account,
        _name: &RuleName,
        _description: &str,
        _priority: u32,
        _level: &RuleLevel,
    ) -> Result<Rule, Box<dyn Error>> {
        Err("rule write failed".into())
    }

    fn make_rule_inactive(&mut self, _rule: &Rule) -> Result<Rule, Box<dyn Error>> {
        Err("rule write failed".into())
    }
}

impl WriteTradingVehicleDB for FailingAccountRead {
    fn create_trading_vehicle(
        &mut self,
        _symbol: &str,
        _isin: Option<&str>,
        _category: &TradingVehicleCategory,
        _broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        Err("trading vehicle write failed".into())
    }

    fn upsert_trading_vehicle(
        &mut self,
        _input: model::database::TradingVehicleUpsert,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        Err("trading vehicle write failed".into())
    }
}

impl ReadBrokerLogsDB for FailingAccountRead {
    fn read_all_logs_for_trade(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<BrokerLog>, Box<dyn Error>> {
        Err("broker log read failed".into())
    }
}

impl WriteBrokerLogsDB for FailingAccountRead {
    fn create_log(&mut self, _log: &str, _trade: &Trade) -> Result<BrokerLog, Box<dyn Error>> {
        Err("broker log write failed".into())
    }
}

impl ReadExecutionDB for FailingAccountRead {
    fn all_trade_executions(&mut self, _trade_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>> {
        Err("execution read failed".into())
    }

    fn all_order_executions(&mut self, _order_id: Uuid) -> Result<Vec<Execution>, Box<dyn Error>> {
        Err("execution read failed".into())
    }

    fn latest_trade_execution_at(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Option<chrono::NaiveDateTime>, Box<dyn Error>> {
        Err("execution read failed".into())
    }
}

impl WriteExecutionDB for FailingAccountRead {
    fn upsert_execution(&mut self, _execution: &Execution) -> Result<Execution, Box<dyn Error>> {
        Err("execution write failed".into())
    }
}

impl ReadMistakeDB for FailingAccountRead {
    fn read_mistakes_for_trade(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<model::Mistake>, Box<dyn Error>> {
        Err("mistake read failed".into())
    }

    fn read_mistakes_for_account_in_period(
        &mut self,
        _account_id: Uuid,
        _start_at: chrono::NaiveDateTime,
        _end_at: chrono::NaiveDateTime,
    ) -> Result<Vec<model::Mistake>, Box<dyn Error>> {
        Err("mistake read failed".into())
    }
}

impl WriteMistakeDB for FailingAccountRead {
    fn create_mistake(
        &mut self,
        _mistake: &model::Mistake,
    ) -> Result<model::Mistake, Box<dyn Error>> {
        Err("mistake write failed".into())
    }
}

impl ReadSessionPlanDB for FailingAccountRead {
    fn read_open_session(
        &mut self,
        _account_id: Uuid,
    ) -> Result<Option<model::SessionPlan>, Box<dyn Error>> {
        Err("session plan read failed".into())
    }

    fn read_session_plans_for_account(
        &mut self,
        _account_id: Uuid,
        _start_at: chrono::NaiveDateTime,
        _end_at: chrono::NaiveDateTime,
    ) -> Result<Vec<model::SessionPlan>, Box<dyn Error>> {
        Err("session plan read failed".into())
    }
}

impl WriteSessionPlanDB for FailingAccountRead {
    fn create_session_plan(
        &mut self,
        _session_plan: &model::SessionPlan,
    ) -> Result<model::SessionPlan, Box<dyn Error>> {
        Err("session plan write failed".into())
    }

    fn close_session_plan(
        &mut self,
        _close: &model::SessionPlanClose,
    ) -> Result<model::SessionPlan, Box<dyn Error>> {
        Err("session plan write failed".into())
    }
}

impl ReadTradeEventDB for FailingAccountRead {
    fn read_trade_events_for_trade(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Vec<TradeEvent>, Box<dyn Error>> {
        Err("trade event read failed".into())
    }
}

impl WriteTradeEventDB for FailingAccountRead {
    fn create_trade_event(&mut self, _event: &TradeEvent) -> Result<TradeEvent, Box<dyn Error>> {
        Err("trade event write failed".into())
    }

    fn delete_trade_event(&mut self, _event_id: Uuid) -> Result<(), Box<dyn Error>> {
        Err("trade event write failed".into())
    }
}

impl ReadTradeGradeDB for FailingAccountRead {
    fn read_latest_for_trade(
        &mut self,
        _trade_id: Uuid,
    ) -> Result<Option<TradeGrade>, Box<dyn Error>> {
        Err("trade grade read failed".into())
    }

    fn read_for_account_days(
        &mut self,
        _account_id: Uuid,
        _days: u32,
    ) -> Result<Vec<TradeGrade>, Box<dyn Error>> {
        Err("trade grade read failed".into())
    }
}

impl WriteTradeGradeDB for FailingAccountRead {
    fn create_trade_grade(&mut self, _grade: &TradeGrade) -> Result<TradeGrade, Box<dyn Error>> {
        Err("trade grade write failed".into())
    }
}

impl ReadLevelDB for FailingAccountRead {
    fn level_for_account(&mut self, _account_id: Uuid) -> Result<Level, Box<dyn Error>> {
        Err("level read failed".into())
    }

    fn level_changes_for_account(
        &mut self,
        _account_id: Uuid,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        Err("level read failed".into())
    }

    fn recent_level_changes(
        &mut self,
        _account_id: Uuid,
        _days: u32,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        Err("level read failed".into())
    }

    fn level_adjustment_rules_for_account(
        &mut self,
        _account_id: Uuid,
    ) -> Result<LevelAdjustmentRules, Box<dyn Error>> {
        Err("level read failed".into())
    }
}

impl WriteLevelDB for FailingAccountRead {
    fn create_default_level(&mut self, _account: &Account) -> Result<Level, Box<dyn Error>> {
        Err("level write failed".into())
    }

    fn update_level(&mut self, _level: &Level) -> Result<Level, Box<dyn Error>> {
        Err("level write failed".into())
    }

    fn create_level_change(
        &mut self,
        _level_change: &LevelChange,
    ) -> Result<LevelChange, Box<dyn Error>> {
        Err("level write failed".into())
    }

    fn upsert_level_adjustment_rules(
        &mut self,
        _account_id: Uuid,
        _rules: &LevelAdjustmentRules,
    ) -> Result<LevelAdjustmentRules, Box<dyn Error>> {
        Err("level write failed".into())
    }
}

impl DistributionRead for FailingAccountRead {
    fn for_account(&mut self, _account_id: Uuid) -> Result<DistributionRules, Box<dyn Error>> {
        Err("distribution read failed".into())
    }

    fn history_for_account(
        &mut self,
        _account_id: Uuid,
    ) -> Result<Vec<DistributionHistory>, Box<dyn Error>> {
        Err("distribution read failed".into())
    }
}

impl DistributionWrite for FailingAccountRead {
    fn create_or_update(
        &mut self,
        _account_id: Uuid,
        _earnings_percent: Decimal,
        _tax_percent: Decimal,
        _reinvestment_percent: Decimal,
        _minimum_threshold: Decimal,
        _configuration_password_hash: &str,
    ) -> Result<DistributionRules, Box<dyn Error>> {
        Err("distribution write failed".into())
    }

    fn create_history(
        &mut self,
        _source_account_id: Uuid,
        _trade_id: Option<Uuid>,
        _original_amount: Decimal,
        _distribution_date: chrono::NaiveDateTime,
        _earnings_amount: Option<Decimal>,
        _tax_amount: Option<Decimal>,
        _reinvestment_amount: Option<Decimal>,
    ) -> Result<DistributionHistory, Box<dyn Error>> {
        Err("distribution write failed".into())
    }

    fn execute_distribution_plan_atomic(
        &mut self,
        _plan: &model::DistributionExecutionPlan,
    ) -> Result<Vec<Uuid>, Box<dyn Error>> {
        Err("distribution write failed".into())
    }
}
