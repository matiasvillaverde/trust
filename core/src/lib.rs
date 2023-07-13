use model::{
    Account, AccountOverview, Broker, BrokerLog, Currency, DatabaseFactory, DraftTrade,
    Environment, Order, OrderStatus, Rule, RuleLevel, RuleName, Status, Trade, TradeOverview,
    TradingVehicle, TradingVehicleCategory, Transaction, TransactionCategory,
};
use rust_decimal::Decimal;
use trade_calculators::QuantityCalculator;
use uuid::Uuid;
use workers::{OrderWorker, OverviewWorker, RuleWorker, TradeWorker, TransactionWorker};

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
            .trade_write()
            .create_trade(draft, &stop, &entry, &target)
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
        // 1. Validate Trade by running rules
        crate::validators::funding::can_fund(trade, &mut *self.factory)?;

        // 2. Fund in case rule succeed
        self.factory
            .trade_write()
            .update_trade_status(Status::Funded, trade)?;

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
        validators::trade::can_submit(trade)?;

        // 2. Submit trade to broker
        let account = self.factory.account_read().id(trade.account_id)?;
        let (log, order_id) = self.broker.submit_trade(trade, &account)?;

        // 3. Save log in the DB
        self.factory
            .log_write()
            .create_log(log.log.as_str(), trade)?;

        // 4. Mark Trade as submitted
        let trade = self
            .factory
            .trade_write()
            .update_trade_status(Status::Submitted, trade)?;

        // 5. Update Orders order to submitted
        self.factory
            .order_write()
            .submit_of(&trade.safety_stop, order_id.stop)?;
        self.factory
            .order_write()
            .submit_of(&trade.entry, order_id.entry)?;
        self.factory
            .order_write()
            .submit_of(&trade.target, order_id.target)?;

        // 6. Read Trade with updated values
        let trade = self.factory.trade_read().read_trade(trade.id)?;

        Ok((trade, log))
    }

    pub fn sync_trade(
        &mut self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
        // 1. Sync Trade with Broker
        let (status, orders, log) = self.broker.sync_trade(trade, account)?;

        // 2. Save log in the DB
        self.factory
            .log_write()
            .create_log(log.log.as_str(), trade)?;

        // 3. Update Orders
        for order in orders.clone() {
            OrderWorker::update_order(&order, &mut *self.factory)?;
        }

        // 4. Update Trade Status
        let trade = self.factory.trade_read().read_trade(trade.id)?; // We need to read the trade again to get the updated orders
        TradeWorker::update_status(&trade, status, &mut *self.factory)?;

        // 5. Update Account Overview
        OverviewWorker::calculate_account(&mut *self.factory, account, &trade.currency)?;

        Ok((status, orders, log))
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
        let (trade, tx_stop) = TradeWorker::stop_executed(trade, fee, self.factory.as_mut())?;
        let (tx_payment, account_overview, trade_overview) =
            TransactionWorker::transfer_payment_from(&trade, self.factory.as_mut())?;
        Ok((tx_stop, tx_payment, trade_overview, account_overview))
    }

    pub fn close_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeOverview, BrokerLog), Box<dyn std::error::Error>> {
        // 1. Verify it can be closed
        validators::trade::can_close(trade)?;

        // 2. Submit a market order to Alpaca
        let account = self.factory.account_read().id(trade.account_id)?;
        let (order, log) = self.broker.close_trade(trade, &account)?;

        // 3. Save log
        self.factory
            .log_write()
            .create_log(log.log.as_str(), trade)?;

        // 4. Update Order Target with the market price and new ID
        OrderWorker::update_order(&order, &mut *self.factory)?;

        // 5. Update Trade Status
        self.factory
            .trade_write()
            .update_trade_status(Status::Canceled, trade)?;

        // 6. Cancel Stop Order
        let mut stop_order = trade.safety_stop.clone();
        stop_order.status = OrderStatus::Canceled;
        self.factory.order_write().update(&stop_order)?;

        Ok((trade.overview.clone(), log))
    }

    pub fn cancel_funded_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeOverview, AccountOverview, Transaction), Box<dyn std::error::Error>> {
        // 1. Verify it can be canceled
        validators::trade::can_cancel(trade)?;

        // 2. Update Trade Status
        self.factory
            .trade_write()
            .update_trade_status(Status::Canceled, trade)?;

        // 3. Transfer funds back to account
        let (tx, account_o, trade_o) =
            TransactionWorker::transfer_payment_from(trade, self.factory.as_mut())?;

        Ok((trade_o, account_o, tx))
    }

    pub fn cancel_submitted_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeOverview, AccountOverview, Transaction), Box<dyn std::error::Error>> {
        // 1. Verify it can be canceled
        validators::trade::can_cancel_submitted(trade)?;

        // 2. Update Trade Status
        self.factory
            .trade_write()
            .update_trade_status(Status::Canceled, trade)?;

        // 3. Transfer funds back to account
        let (tx, account_o, trade_o) =
            TransactionWorker::transfer_payment_from(trade, self.factory.as_mut())?;

        Ok((trade_o, account_o, tx))
    }

    pub fn target_acquired(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<
        (Transaction, Transaction, TradeOverview, AccountOverview),
        Box<dyn std::error::Error>,
    > {
        let (trade, tx_target) = TradeWorker::target_executed(trade, fee, self.factory.as_mut())?;
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
