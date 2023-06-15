use rust_decimal::Decimal;
use trade_calculators::QuantityCalculator;
use trust_model::{
    Account, AccountOverview, Broker, BrokerLog, Currency, DatabaseFactory, DraftTrade,
    Environment, Order, Rule, RuleLevel, RuleName, Status, Trade, TradeOverview, TradingVehicle,
    TradingVehicleCategory, Transaction, TransactionCategory,
};
use uuid::Uuid;
use validators::RuleValidator;
use workers::{OrderWorker, OverviewWorker, RuleWorker, TradeWorker, TransactionWorker};

pub struct TrustFacade {
    factory: Box<dyn DatabaseFactory>,
    broker: Box<dyn Broker>,
}

/// Trust is the main entry point for interacting with the trust-core library.
/// It is a facade that provides a simple interface for interacting with the
/// trust-core library.
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
        self.factory.write_account_db().create_account(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
        )
    }

    pub fn search_account(&mut self, name: &str) -> Result<Account, Box<dyn std::error::Error>> {
        self.factory.read_account_db().read_account(name)
    }

    pub fn search_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
        self.factory.read_account_db().read_all_accounts()
    }

    pub fn search_all_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.factory.read_rule_db().read_all_rules(account_id)
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
            .read_account_overview_db()
            .read_account_overview_currency(account_id, currency)
    }

    pub fn search_all_overviews(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn std::error::Error>> {
        self.factory
            .read_account_overview_db()
            .read_account_overview(account_id)
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
        self.factory.write_rule_db().make_rule_inactive(rule)
    }

    pub fn search_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.factory.read_rule_db().read_all_rules(account_id)
    }

    pub fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn std::error::Error>> {
        self.factory
            .write_trading_vehicle_db()
            .create_trading_vehicle(symbol, isin, category, broker)
    }

    pub fn search_trading_vehicles(
        &mut self,
    ) -> Result<Vec<TradingVehicle>, Box<dyn std::error::Error>> {
        self.factory
            .read_trading_vehicle_db()
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
        let stop = OrderWorker::create_stop(
            trade.trading_vehicle.id,
            trade.quantity,
            stop_price,
            &trade.currency,
            &trade.category,
            &mut *self.factory,
        )?;

        let entry = OrderWorker::create_entry(
            trade.trading_vehicle.id,
            trade.quantity,
            entry_price,
            &trade.currency,
            &trade.category,
            &mut *self.factory,
        )?;

        let target = OrderWorker::create_target(
            trade.trading_vehicle.id,
            trade.quantity,
            target_price,
            &trade.currency,
            &trade.category,
            &mut *self.factory,
        )?;

        let draft = DraftTrade {
            account: trade.account,
            trading_vehicle: trade.trading_vehicle,
            quantity: trade.quantity,
            currency: trade.currency,
            category: trade.category,
        };

        self.factory
            .write_trade_db()
            .create_trade(draft, &stop, &entry, &target)
    }

    pub fn search_trades(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
        self.factory
            .read_trade_db()
            .read_trades_with_status(account_id, status)
    }

    // Trade Steps

    pub fn fund_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(Trade, Transaction, AccountOverview, TradeOverview), Box<dyn std::error::Error>>
    {
        // 1. Validate Trade by running rules
        RuleValidator::validate_trade(trade, &mut *self.factory)?;
        // 2. Approve in case rule succeed
        self.factory.write_trade_db().fund_trade(trade)?;
        // 3. Create transaction to fund the trade
        let (transaction, account_overview, trade_overview) =
            TransactionWorker::transfer_to_fund_trade(trade, &mut *self.factory)?;
        Ok((trade.clone(), transaction, account_overview, trade_overview))
    }

    pub fn submit_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
        // 1. Validate Trade
        RuleValidator::validate_submit(trade)?;

        // 2. Submit trade to broker
        let account = self
            .factory
            .read_account_db()
            .read_account_id(trade.account_id)?;
        let (log, order_id) = self.broker.submit_trade(trade, &account)?;

        // 3. Save log in the DB
        self.factory
            .write_broker_log_db()
            .create_log(log.log.as_str(), trade)?;

        // 4. Mark Trade as submitted
        let trade = self.factory.write_trade_db().submit_trade(trade)?;

        // 5. Update Orders order to submitted
        self.factory
            .write_order_db()
            .record_submit(&trade.safety_stop, order_id.stop)?;
        self.factory
            .write_order_db()
            .record_submit(&trade.entry, order_id.entry)?;
        self.factory
            .write_order_db()
            .record_submit(&trade.target, order_id.target)?;

        // 6. Read Trade with updated values
        let trade = self.factory.read_trade_db().read_trade(trade.id)?;

        Ok((trade, log))
    }

    pub fn sync_trade(
        &mut self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>), Box<dyn std::error::Error>> {
        // 1. Sync Trade with Broker
        let (status, orders, log) = self.broker.sync_trade(trade, account)?;

        // 2. Save log in the DB
        self.factory
            .write_broker_log_db()
            .create_log(log.log.as_str(), trade)?;

        // 3. Update Orders
        for order in orders.clone() {
            OrderWorker::update_order(&order, &mut *self.factory)?;
        }

        // 4. Update Trade Status
        let trade = self.factory.read_trade_db().read_trade(trade.id)?; // We need to read the trade again to get the updated orders
        TradeWorker::update_status(&trade, status, &mut *self.factory)?;

        // 5. Update Account Overview
        OverviewWorker::update_account_overview(&mut *self.factory, account, &trade.currency)?;

        Ok((status, orders))
    }

    pub fn fill_trade(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<(Trade, Transaction), Box<dyn std::error::Error>> {
        TradeWorker::fill_trade(trade, fee, self.factory.as_mut())
    }

    pub fn stop_trade(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<
        (Transaction, Transaction, TradeOverview, AccountOverview),
        Box<dyn std::error::Error>,
    > {
        let (trade, tx_stop) =
            TradeWorker::update_trade_stop_executed(trade, fee, self.factory.as_mut())?;
        let (tx_payment, account_overview, trade_overview) =
            TransactionWorker::transfer_payment_from(&trade, self.factory.as_mut())?;
        Ok((tx_stop, tx_payment, trade_overview, account_overview))
    }

    pub fn target_acquired(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<
        (Transaction, Transaction, TradeOverview, AccountOverview),
        Box<dyn std::error::Error>,
    > {
        let (trade, tx_target) =
            TradeWorker::update_trade_target_executed(trade, fee, self.factory.as_mut())?;
        let (tx_payment, account_overview, trade_overview) =
            TransactionWorker::transfer_payment_from(&trade, self.factory.as_mut())?;
        Ok((tx_target, tx_payment, trade_overview, account_overview))
    }
}

mod account_calculators;
mod mocks;
mod trade_calculators;
mod validators;
mod workers;
