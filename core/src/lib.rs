use model::{
    Account, AccountOverview, Broker, BrokerLog, Currency, DatabaseFactory, DraftTrade,
    Environment, Order, OrderStatus, Rule, RuleLevel, RuleName, Status, Trade, TradeOverview,
    TradingVehicle, TradingVehicleCategory, Transaction, TransactionCategory,
};
use rust_decimal::Decimal;
use trade_calculators::QuantityCalculator;
use uuid::Uuid;
use workers::{
    OrderWorker, OverviewWorker, RuleWorker, TradeAction, TradeLifecycle, TransactionWorker,
};

pub struct TrustFacade {
    factory: Box<dyn DatabaseFactory>,
    broker: Box<dyn Broker>,
}

/// Trust is the main entry point for interacting with the core library.
/// It is a facade that provides a simple interface for interacting with the
/// core library.
impl TrustFacade {
    /// Creates a new instance of Trust.
    pub fn new(factory: Box<dyn DatabaseFactory>, broker: Box<dyn Broker>) -> Self {
        TrustFacade { factory, broker }
    }

    /// Creates a new account.
    pub fn create_account(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        self.factory.account_write().create(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
        )
    }

    pub fn search_account(&mut self, name: &str) -> Result<Account, Box<dyn std::error::Error>> {
        self.factory.account_read().for_name(name)
    }

    pub fn search_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
        self.factory.account_read().all()
    }

    pub fn search_all_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.factory.rule_read().read_all_rules(account_id)
    }

    pub fn create_transaction(
        &mut self,
        account: &Account,
        category: &TransactionCategory,
        amount: Decimal,
        currency: &Currency,
    ) -> Result<(Transaction, AccountOverview), Box<dyn std::error::Error>> {
        TransactionWorker::create(&mut *self.factory, category, amount, currency, account.id)
    }

    pub fn search_overview(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn std::error::Error>> {
        self.factory
            .account_overview_read()
            .for_currency(account_id, currency)
    }

    pub fn search_all_overviews(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn std::error::Error>> {
        self.factory.account_overview_read().for_account(account_id)
    }

    pub fn create_rule(
        &mut self,
        account: &Account,
        name: &RuleName,
        description: &str,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn std::error::Error>> {
        RuleWorker::create_rule(&mut *self.factory, account, name, description, level)
    }

    pub fn deactivate_rule(&mut self, rule: &Rule) -> Result<Rule, Box<dyn std::error::Error>> {
        self.factory.rule_write().make_rule_inactive(rule)
    }

    pub fn search_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.factory.rule_read().read_all_rules(account_id)
    }

    pub fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn std::error::Error>> {
        self.factory
            .trading_vehicle_write()
            .create_trading_vehicle(symbol, isin, category, broker)
    }

    pub fn search_trading_vehicles(
        &mut self,
    ) -> Result<Vec<TradingVehicle>, Box<dyn std::error::Error>> {
        self.factory
            .trading_vehicle_read()
            .read_all_trading_vehicles()
    }

    pub fn calculate_maximum_quantity(
        &mut self,
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        QuantityCalculator::maximum_quantity(
            account_id,
            entry_price,
            stop_price,
            currency,
            &mut *self.factory,
        )
    }

    pub fn create_trade(
        &mut self,
        trade: DraftTrade,
        stop_price: Decimal,
        entry_price: Decimal,
        target_price: Decimal,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        TradeAction::create_trade(
            trade,
            stop_price,
            entry_price,
            target_price,
            &mut *self.factory,
        )
    }

    pub fn search_trades(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
        self.factory
            .trade_read()
            .read_trades_with_status(account_id, status)
    }

    // Trade Steps

    pub fn fund_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(Trade, Transaction, AccountOverview, TradeOverview), Box<dyn std::error::Error>>
    {
        TradeLifecycle::fund_trade(trade, &mut *self.factory)
    }

    pub fn submit_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
        TradeLifecycle::submit_trade(trade, &mut *self.factory, &mut *self.broker)
    }

    pub fn sync_trade(
        &mut self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
        TradeLifecycle::sync_trade(trade, account, &mut *self.factory, &mut *self.broker)
    }

    pub fn fill_trade(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<(Trade, Transaction), Box<dyn std::error::Error>> {
        TradeAction::fill_trade(trade, fee, self.factory.as_mut())
    }

    pub fn stop_trade(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<
        (Transaction, Transaction, TradeOverview, AccountOverview),
        Box<dyn std::error::Error>,
    > {
        TradeAction::stop_trade(trade, fee, &mut *self.factory)
    }

    pub fn close_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeOverview, BrokerLog), Box<dyn std::error::Error>> {
        TradeLifecycle::close_trade(trade, &mut *self.factory, &mut *self.broker)
    }

    pub fn cancel_funded_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeOverview, AccountOverview, Transaction), Box<dyn std::error::Error>> {
        TradeAction::cancel_funded_trade(trade, &mut *self.factory)
    }

    pub fn cancel_submitted_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeOverview, AccountOverview, Transaction), Box<dyn std::error::Error>> {
        TradeAction::cancel_submitted_trade(trade, &mut *self.factory, &mut *self.broker)
    }

    pub fn target_acquired(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<
        (Transaction, Transaction, TradeOverview, AccountOverview),
        Box<dyn std::error::Error>,
    > {
        TradeAction::target_acquired(trade, fee, &mut *self.factory)
    }

    pub fn modify_stop(
        &mut self,
        trade: &Trade,
        account: &Account,
        new_stop_price: Decimal,
    ) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
        TradeAction::modify_stop(
            trade,
            account,
            new_stop_price,
            &mut *self.broker,
            &mut *self.factory,
        )
    }
}

mod account_calculators;
mod mocks;
mod trade_calculators;
mod validators;
mod workers;
