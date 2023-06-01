use rust_decimal::Decimal;
use trade_calculators::QuantityCalculator;
use trust_model::{
    Account, AccountOverview, Currency, DatabaseFactory, Rule, RuleLevel, RuleName, Trade,
    TradeCategory, TradeOverview, TradingVehicle, TradingVehicleCategory, Transaction,
    TransactionCategory,
};
use uuid::Uuid;
use validators::RuleValidator;
use workers::{OrderWorker, RuleWorker, TradeWorker, TransactionWorker};

pub struct TrustFacade {
    factory: Box<dyn DatabaseFactory>,
}

/// Trust is the main entry point for interacting with the trust-core library.
/// It is a facade that provides a simple interface for interacting with the
/// trust-core library.
impl TrustFacade {
    /// Creates a new instance of Trust.
    pub fn new(factory: Box<dyn DatabaseFactory>) -> Self {
        TrustFacade { factory }
    }

    /// Creates a new account.
    pub fn create_account(
        &mut self,
        name: &str,
        description: &str,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        self.factory
            .write_account_db()
            .create_account(name, description)
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

    pub fn create_trade(&mut self, trade: DraftTrade) -> Result<Trade, Box<dyn std::error::Error>> {
        let trading_vehicle = self
            .factory
            .read_trading_vehicle_db()
            .read_trading_vehicle(trade.trading_vehicle_id)?;

        let stop = OrderWorker::create_stop(
            trade.trading_vehicle_id,
            trade.quantity,
            trade.stop_price,
            &trade.currency,
            &trade.category,
            &mut *self.factory,
        )?;

        let entry = OrderWorker::create_entry(
            trade.trading_vehicle_id,
            trade.quantity,
            trade.entry_price,
            &trade.currency,
            &trade.category,
            &mut *self.factory,
        )?;

        let new_trade = self.factory.write_trade_db().create_trade(
            &trade.category,
            &trade.currency,
            &trading_vehicle,
            &stop,
            &entry,
            &trade.account,
        )?;

        let mut targets = Vec::new();
        for target in trade.targets {
            let draft = DraftTarget {
                target_price: target.target_price,
                quantity: target.quantity,
                order_price: target.order_price,
            };

            let target = OrderWorker::create_target(draft, &new_trade, &mut *self.factory)?;
            targets.push(target);
        }

        // We need to read again the trade with the recently added targets
        let new_trade = self.factory.read_trade_db().read_trade(new_trade.id)?;

        Ok(new_trade)
    }

    pub fn search_new_trades(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
        self.factory.read_trade_db().read_all_new_trades(account_id)
    }

    pub fn search_approved_trades(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
        self.factory.read_trade_db().all_approved_trades(account_id)
    }

    pub fn search_open_trades(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
        self.factory.read_trade_db().all_open_trades(account_id)
    }

    pub fn approve(
        &mut self,
        trade: &Trade,
    ) -> Result<(Trade, Transaction, AccountOverview, TradeOverview), Box<dyn std::error::Error>>
    {
        // 1. Validate Trade by running rules
        RuleValidator::validate_trade(trade, &mut *self.factory)?;
        // 2. Approve in case rule succeed
        self.factory.write_trade_db().approve_trade(trade)?;
        // 3. Create transaction to fund the trade
        let (transaction, account_overview, trade_overview) =
            TransactionWorker::transfer_to_fund_trade(trade, &mut *self.factory)?;
        Ok((trade.clone(), transaction, account_overview, trade_overview))
    }

    pub fn open_trade(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        TradeWorker::open_trade(trade, fee, self.factory.as_mut())
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

pub struct DraftTrade {
    pub account: Account,
    pub trading_vehicle_id: Uuid,
    pub quantity: i64,
    pub currency: Currency,
    pub category: TradeCategory,
    pub stop_price: Decimal,
    pub entry_price: Decimal,
    pub targets: Vec<DraftTarget>,
}

pub struct DraftTarget {
    pub target_price: Decimal,
    pub quantity: i64,
    pub order_price: Decimal,
}
