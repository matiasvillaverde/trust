use tabled::settings::style::Style;
use tabled::Table;
use tabled::Tabled;
use trust_model::Transaction;

#[derive(Tabled)]
pub struct TransactionView {
    pub account_name: String,
    pub category: String,
    pub amount: String,
    pub currency: String,
}

impl TransactionView {
    fn new(tx: &Transaction, account_name: &str) -> TransactionView {
        TransactionView {
            account_name: crate::views::uppercase_first(account_name),
            category: tx.category.to_string(),
            amount: tx.price.amount.to_string(),
            currency: tx.price.currency.to_string(),
        }
    }

    pub fn display(tx: &Transaction, account_name: &str) {
        TransactionView::display_transactions(vec![tx], account_name);
        println!();
    }

    pub fn display_transactions(txs: Vec<&Transaction>, account_name: &str) {
        let views: Vec<TransactionView> = txs
            .into_iter()
            .map(|x: &Transaction| TransactionView::new(x, account_name))
            .collect();
        let mut table = Table::new(views);
        table.with(Style::modern());
        println!("{}", table);
    }
}
