use rust_decimal::{prelude::ToPrimitive, Decimal};
use rust_decimal_macros::dec;
use trust_model::{
    Account, AccountOverview, Currency, Database, Rule, RuleLevel, RuleName, TradeCategory,
    TradingVehicle, TradingVehicleCategory, Transaction, TransactionCategory,
};
use uuid::Uuid;
use workers::{RuleWorker, TransactionWorker};

pub struct Trust {
    database: Box<dyn Database>,
}

/// Trust is the main entry point for interacting with the trust-core library.
/// It is a facade that provides a simple interface for interacting with the
/// trust-core library.
impl Trust {
    /// Creates a new instance of Trust.
    pub fn new(database: Box<dyn Database>) -> Self {
        Trust { database }
    }

    /// Creates a new account.
    pub fn create_account(
        &mut self,
        name: &str,
        description: &str,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        self.database.new_account(name, description)
    }

    pub fn search_account(&mut self, name: &str) -> Result<Account, Box<dyn std::error::Error>> {
        self.database.read_account(name)
    }

    pub fn search_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
        self.database.read_all_accounts()
    }

    pub fn search_all_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.database.read_all_rules(account_id)
    }

    pub fn create_transaction(
        &mut self,
        account: &Account,
        category: &TransactionCategory,
        amount: Decimal,
        currency: &Currency,
    ) -> Result<(Transaction, AccountOverview), Box<dyn std::error::Error>> {
        TransactionWorker::create(&mut *self.database, category, amount, currency, account.id)
    }

    pub fn search_overview(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn std::error::Error>> {
        self.database
            .read_account_overview_currency(account_id, currency)
    }

    pub fn search_all_overviews(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn std::error::Error>> {
        self.database.read_account_overview(account_id)
    }

    pub fn create_rule(
        &mut self,
        account: &Account,
        name: &RuleName,
        description: &str,
        priority: u32,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn std::error::Error>> {
        RuleWorker::create_rule(
            &mut *self.database,
            account,
            name,
            description,
            priority,
            level,
        )
    }

    pub fn make_rule_inactive(&mut self, rule: &Rule) -> Result<Rule, Box<dyn std::error::Error>> {
        self.database.make_rule_inactive(rule)
    }

    pub fn read_all_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.database.read_all_rules(account_id)
    }

    pub fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn std::error::Error>> {
        self.database
            .create_trading_vehicle(symbol, isin, category, broker)
    }

    pub fn read_all_trading_vehicles(
        &mut self,
    ) -> Result<Vec<TradingVehicle>, Box<dyn std::error::Error>> {
        self.database.read_all_trading_vehicles()
    }

    pub fn maximum_quantity(
        &mut self,
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        _TradeCategory: &TradeCategory,
        currency: &Currency,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let overview = self
            .database
            .read_account_overview_currency(account_id, currency)?;
        let available = overview.total_available.amount;

        // Get rules by priority
        let mut rules = self.database.read_all_rules(account_id)?;
        rules.sort_by(|a, b| a.priority.cmp(&b.priority));

        // match rules by name

        for rule in rules {
            match rule.name {
                RuleName::RiskPerMonth(_risk) => {
                    unimplemented!("risk_per_month")
                }
                RuleName::RiskPerTrade(risk) => {
                    let risk_per_trade =
                        self.risk_per_trade(available, entry_price, stop_price, risk)?;
                    return Ok(risk_per_trade);
                }
            }
        }

        // If there are no rules, return the maximum quantity based on available funds
        Ok((available / entry_price).to_i64().unwrap())
    }

    fn risk_per_trade(
        &mut self,
        available: Decimal,
        entry_price: Decimal,
        stop_price: Decimal,
        risk: f32,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let risk = available * (Decimal::from_f32_retain(risk).unwrap() / dec!(100.0));
        let risk_per_trade = risk / (entry_price - stop_price);
        let risk_per_trade = risk_per_trade.to_i64().unwrap();
        Ok(risk_per_trade)
    }
}

mod validators;
mod workers;
