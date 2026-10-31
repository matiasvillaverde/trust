use core::{services::TaxRateOverrides, TrustFacade};
use db_sqlite::SqliteDatabase;
use model::{
    Currency, DraftTrade, Environment, TradeCategory, TradingVehicleCategory, TransactionCategory,
};
use rust_decimal_macros::dec;

#[test]
fn one_and_two_percent_have_consistent_tax_liability_on_realized_profit() {
    for (points, rate, tax) in [
        (dec!(0.25), dec!(0.0025), dec!(0.25)),
        (dec!(1), dec!(0.01), dec!(1)),
        (dec!(2), dec!(0.02), dec!(2)),
        (dec!(100), dec!(1), dec!(100)),
    ] {
        let mut trust =
            TrustFacade::new_with_brokers(Box::new(SqliteDatabase::new_in_memory()), vec![]);
        let account = trust
            .create_account("explicit-tax", "", Environment::Paper, points, dec!(0))
            .unwrap();
        trust
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                dec!(10000),
                &Currency::USD,
            )
            .unwrap();
        let instrument = trust
            .create_trading_vehicle("TAX", None, &TradingVehicleCategory::Stock, "ibkr")
            .unwrap();
        let draft = DraftTrade {
            account: account.clone(),
            trading_vehicle: instrument,
            quantity: dec!(10),
            currency: Currency::USD,
            category: TradeCategory::Long,
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
        };
        let trade = trust
            .create_trade(draft, dec!(95), dec!(100), dec!(110))
            .unwrap();
        let (mut funded, _, _, _) = trust.fund_trade(&trade).unwrap();
        funded.entry.average_filled_price = Some(dec!(100));
        let (mut filled, _) = trust.fill_trade(&funded, dec!(0)).unwrap();
        filled.target.average_filled_price = Some(dec!(110));
        trust.target_acquired(&filled, dec!(0)).unwrap();
        let report = trust
            .tax_report(Some(account.id), TaxRateOverrides::default())
            .unwrap();
        assert_eq!(report.taxable_trade_count, 1);
        assert_eq!(report.gross_realized_gain, dec!(100));
        assert_eq!(report.trades[0].tax_rate, rate);
        assert_eq!(report.estimated_net_tax_liability, tax);
    }
}
