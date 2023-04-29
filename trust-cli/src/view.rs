use tabled::settings::style::Style;
use tabled::{Table, Tabled};
use trust_model::Account;

#[derive(Tabled)]
pub struct AccountView {
    pub name: String,
    pub description: String,
}

impl AccountView {
    pub fn new(account: Account) -> AccountView {
        AccountView {
            name: account.name,
            description: account.description,
        }
    }

    pub fn display_account(a: Account) {
        AccountView::display_accounts(vec![a]);
    }

    pub fn display_accounts(accounts: Vec<Account>) {
        let views: Vec<AccountView> = accounts.into_iter().map(AccountView::new).collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}
