use trust_model::{
    Account, AccountOverview, Currency, Database, Order, OrderAction, Price, Rule, RuleLevel,
    RuleName, Target, Trade, TradeCategory, TradeOverview, TradingVehicle, TradingVehicleCategory,
    Transaction, TransactionCategory,
};

use rust_decimal::Decimal;
use std::error::Error;
use uuid::Uuid;

#[derive(Default)]
pub struct MemoryDatabase {
    accounts: Vec<Account>,
    account_overviews: Vec<AccountOverview>,
    prices: Vec<Price>,
    transactions: Vec<Transaction>,
    // trades: Vec<Trade>,
    // targets: Vec<Target>,
    // rules: Vec<Rule>,
    // trading_vehicles: Vec<TradingVehicle>,
    // trade_overviews: Vec<TradeOverview>,
    // orders: Vec<Order>,
}

impl Database for MemoryDatabase {
    fn read_account_id(&mut self, account_id: Uuid) -> Result<Account, Box<dyn Error>> {
        self.accounts
            .clone()
            .into_iter()
            .find(|a| a.id == account_id)
            .ok_or("Account not found".into())
    }

    fn read_account(&mut self, name: &str) -> Result<Account, Box<dyn Error>> {
        self.accounts
            .clone()
            .into_iter()
            .find(|a| a.name == name)
            .ok_or("Account not found".into())
    }

    fn read_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn Error>> {
        Ok(self.accounts.clone())
    }

    fn new_account(&mut self, name: &str, description: &str) -> Result<Account, Box<dyn Error>> {
        let account = Account::new(name, description);
        self.accounts.push(account.clone());
        Ok(account)
    }

    fn new_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let account_overview = AccountOverview::new(account.id, currency);
        self.account_overviews.push(account_overview);
        Ok(account_overview)
    }

    fn read_account_overview(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>> {
        Ok(self
            .account_overviews
            .clone()
            .into_iter()
            .filter(|a| a.account_id == account_id)
            .collect())
    }

    fn read_account_overview_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let account_overviews: Vec<AccountOverview> = self
            .account_overviews
            .clone()
            .into_iter()
            .filter(|a| a.account_id == account_id)
            .collect();
        let account_overview = account_overviews
            .into_iter()
            .find(|a| a.currency == *currency);
        match account_overview {
            Some(a) => Ok(a),
            None => Err("Account overview not found".into()),
        }
    }

    fn new_price(&mut self, currency: &Currency, amount: Decimal) -> Result<Price, Box<dyn Error>> {
        let price = Price::new(currency, amount);
        self.prices.push(price);
        Ok(price)
    }

    fn read_price(&mut self, _id: Uuid) -> Result<Price, Box<dyn Error>> {
        self.prices
            .clone()
            .into_iter()
            .find(|p| p.id == _id)
            .ok_or("Price not found".into())
    }

    fn new_transaction(
        &mut self,
        account: &Account,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        let transaction =
            Transaction::new(account.id, category, currency, Price::new(currency, amount));
        self.transactions.push(transaction.clone());
        Ok(transaction)
    }

    fn all_trade_transactions_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let transactions: Vec<Transaction> = self
            .transactions
            .clone()
            .into_iter()
            .filter(|t| t.account_id == account_id)
            .filter(|t| t.currency == *currency)
            .filter(|t| t.category != TransactionCategory::OutputTax)
            .collect();

        let transactions = transactions
            .into_iter()
            .filter(|t| !matches!(t.category, TransactionCategory::InputTax(_)))
            .collect();

        Ok(transactions)
    }

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>> {
        unimplemented!()
    }

    fn all_open_trades(
        &mut self,
        _account_id: Uuid,
        _currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>> {
        unimplemented!()
    }

    fn create_rule(
        &mut self,
        _account: &Account,
        _name: &RuleName,
        _description: &str,
        _priority: u32,
        _level: &RuleLevel,
    ) -> Result<Rule, Box<dyn Error>> {
        unimplemented!()
    }

    fn read_all_rules(&mut self, _account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>> {
        unimplemented!()
    }

    fn make_rule_inactive(&mut self, _rule: &Rule) -> Result<Rule, Box<dyn Error>> {
        unimplemented!()
    }

    fn rule_for_account(
        &mut self,
        _account_id: Uuid,
        _name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>> {
        unimplemented!()
    }

    fn create_trading_vehicle(
        &mut self,
        _symbol: &str,
        _isin: &str,
        _category: &TradingVehicleCategory,
        _broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        unimplemented!()
    }

    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        unimplemented!()
    }

    fn read_trading_vehicle(&mut self, _id: Uuid) -> Result<TradingVehicle, Box<dyn Error>> {
        unimplemented!()
    }

    fn create_trade(
        &mut self,
        _category: &TradeCategory,
        _currency: &Currency,
        _trading_vehicle: &TradingVehicle,
        _safety_stop: &Order,
        _entry: &Order,
        _account: &Account,
    ) -> Result<Trade, Box<dyn Error>> {
        unimplemented!()
    }

    fn read_trade(&mut self, _trade_id: Uuid) -> Result<Trade, Box<dyn Error>> {
        unimplemented!()
    }

    fn read_all_new_trades(&mut self, _account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
        unimplemented!()
    }

    fn update_account_overview_trade(
        &mut self,
        _account: &Account,
        _currency: &Currency,
        _amount: Decimal,
        _price: Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        unimplemented!()
    }

    fn update_account_overview(
        &mut self,
        _account: &Account,
        _currency: &Currency,
        _amount: Decimal,
        _price: Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        unimplemented!()
    }

    fn create_order(
        &mut self,
        _trading_vehicle: &TradingVehicle,
        _quantity: i64,
        _price: Decimal,
        _currency: &Currency,
        _action: &OrderAction,
    ) -> Result<Order, Box<dyn Error>> {
        unimplemented!()
    }

    fn create_target(
        &mut self,
        _target_price: Decimal,
        _currency: &Currency,
        _order: &Order,
        _trade: &Trade,
    ) -> Result<Target, Box<dyn Error>> {
        unimplemented!()
    }

    fn approve_trade(&mut self, _trade: &Trade) -> Result<Trade, Box<dyn Error>> {
        unimplemented!()
    }

    fn update_trade_overview(
        &mut self,
        _trade: &Trade,
        _price: Decimal,
    ) -> Result<TradeOverview, Box<dyn Error>> {
        unimplemented!()
    }
}
