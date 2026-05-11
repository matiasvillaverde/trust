use model::{Account, AccountBalance};
use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;

#[derive(Tabled)]
pub struct AccountView {
    pub name: String,
    pub description: String,
    pub env: String,
    pub broker: String,
    pub broker_account: String,
}

impl AccountView {
    fn new(account: Account) -> AccountView {
        AccountView {
            name: account.name,
            description: account.description,
            env: account.environment.to_string(),
            broker: account.broker_kind.to_string(),
            broker_account: account.broker_account_id.unwrap_or_else(|| "-".to_string()),
        }
    }

    pub fn display_account(a: Account) {
        println!();
        println!("Account: {}", a.id);
        AccountView::display_accounts(vec![a]);
        println!();
    }

    pub fn display_accounts(accounts: Vec<Account>) {
        let views: Vec<AccountView> = accounts.into_iter().map(AccountView::new).collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{table}");
    }
}

#[derive(Tabled)]
pub struct AccountBalanceView {
    pub account_name: String,
    pub total_balance: String,
    pub total_available: String,
    pub total_in_trade: String,
    pub taxed: String,
    pub currency: String,
}

impl AccountBalanceView {
    fn new(balance: AccountBalance, account_name: &str) -> AccountBalanceView {
        let basis = balance.total_balance;
        AccountBalanceView {
            account_name: crate::views::uppercase_first(account_name),
            total_balance: crate::zen::amount_share(balance.total_balance, basis),
            total_available: crate::zen::amount_share(balance.total_available, basis),
            total_in_trade: crate::zen::amount_share(balance.total_in_trade, basis),
            taxed: crate::zen::amount_share(balance.taxed, basis),
            currency: balance.currency.to_string(),
        }
    }

    pub fn display(balance: AccountBalance, account_name: &str) {
        println!();
        println!("Account balance: {}", balance.id);
        AccountBalanceView::display_balances(vec![balance], account_name);
        println!();
    }

    pub fn display_balances(balances: Vec<AccountBalance>, account_name: &str) {
        let views: Vec<AccountBalanceView> = balances
            .into_iter()
            .map(|x| AccountBalanceView::new(x, account_name))
            .collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{table}");
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountBalanceView, AccountView};
    use model::{Account, AccountBalance, Environment};
    use rust_decimal_macros::dec;

    #[test]
    fn account_view_new_maps_fields() {
        let account = Account {
            name: "paper".to_string(),
            description: "test account".to_string(),
            environment: Environment::Paper,
            ..Default::default()
        };

        let view = AccountView::new(account);
        assert_eq!(view.name, "paper");
        assert_eq!(view.description, "test account");
        assert_eq!(view.env, "paper");
        assert_eq!(view.broker, "alpaca");
        assert_eq!(view.broker_account, "-");
    }

    #[test]
    fn account_balance_view_new_formats_and_capitalizes() {
        crate::zen::set_enabled(false);
        let balance = AccountBalance {
            total_balance: dec!(1000),
            total_available: dec!(900),
            total_in_trade: dec!(100),
            taxed: dec!(10),
            ..Default::default()
        };

        let view = AccountBalanceView::new(balance, "main");
        assert_eq!(view.account_name, "Main");
        assert_eq!(view.total_balance, "1000");
        assert_eq!(view.total_available, "900");
        assert_eq!(view.total_in_trade, "100");
        assert_eq!(view.taxed, "10");
    }

    #[test]
    fn account_balance_view_new_uses_percentages_in_zen_mode() {
        crate::zen::set_enabled(true);
        let balance = AccountBalance {
            total_balance: dec!(1000),
            total_available: dec!(900),
            total_in_trade: dec!(100),
            taxed: dec!(10),
            ..Default::default()
        };

        let view = AccountBalanceView::new(balance, "main");
        assert_eq!(view.total_balance, "100.0%");
        assert_eq!(view.total_available, "90.0%");
        assert_eq!(view.total_in_trade, "10.0%");
        assert_eq!(view.taxed, "1.0%");
        crate::zen::set_enabled(false);
    }

    #[test]
    fn display_functions_run_for_smoke_coverage() {
        crate::zen::set_enabled(false);
        AccountView::display_account(Account::default());
        AccountView::display_accounts(vec![Account::default()]);
        AccountBalanceView::display_balances(vec![AccountBalance::default()], "primary");
    }
}
