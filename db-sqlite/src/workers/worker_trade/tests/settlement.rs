use super::*;
use chrono::Datelike;

#[test]
fn settlement_dates_round_trip_across_weekends_months_years_and_leap_days() {
    use TradingVehicleCategory::{Bond, Crypto, Etf, Fiat, Stock};
    let (database, connection) = create_database_with_connection();
    let account = create_account(&database, "settlement-calendar");
    let mut connection = connection.lock().unwrap();
    // These are Trust's weekday-only default conventions, not an exchange holiday calendar.
    for (date, next_weekday, second_weekday) in [
        ("2026-05-15", "2026-05-18", "2026-05-19"),
        ("2026-05-16", "2026-05-18", "2026-05-19"),
        ("2026-05-17", "2026-05-18", "2026-05-19"),
        ("2026-07-31", "2026-08-03", "2026-08-04"),
        ("2027-12-31", "2028-01-03", "2028-01-04"),
        ("2024-02-28", "2024-02-29", "2024-03-01"),
    ] {
        for (category, expected) in [
            (Stock, next_weekday),
            (Etf, next_weekday),
            (Bond, second_weekday),
            (Crypto, date),
            (Fiat, date),
        ] {
            let symbol = format!("{category}-{date}");
            let seed = create_trade_with_category(
                &mut connection,
                &account,
                &symbol,
                Currency::USD,
                category,
            );
            let created_at = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(23, 59, 59)
                .unwrap();
            let draft = DraftTrade {
                account: account.clone(),
                trading_vehicle: seed.trading_vehicle.clone(),
                quantity: dec!(10),
                currency: Currency::USD,
                category: TradeCategory::Long,
                thesis: None,
                sector: None,
                asset_class: None,
                context: None,
            };
            let trade = WorkerTrade::create_at(
                &mut connection,
                draft,
                &seed.safety_stop,
                &seed.entry,
                &seed.target,
                created_at,
            )
            .unwrap();
            let expected = Some(NaiveDate::parse_from_str(expected, "%Y-%m-%d").unwrap());
            assert_eq!(trade.settlement_date, expected, "{symbol}");
            assert_eq!(trade.created_at, created_at);
            let persisted = WorkerTrade::read_trade(&mut connection, trade.id).unwrap();
            assert_eq!(persisted, trade, "persisted calendar result for {symbol}");
        }
    }
}

#[test]
fn normal_creation_persists_settlement_using_the_creation_date() {
    use TradingVehicleCategory::{Bond, Crypto, Etf, Fiat, Stock};
    let (database, connection) = create_database_with_connection();
    let account = create_account(&database, "settlement-current-clock");
    let mut connection = connection.lock().unwrap();
    for category in [Stock, Etf, Bond, Crypto, Fiat] {
        let trade = create_trade_with_category(
            &mut connection,
            &account,
            &format!("NOW-{category}"),
            Currency::USD,
            category,
        );
        let weekday = trade.created_at.weekday().num_days_from_monday() as usize;
        // Independent weekday offsets, with no call to the settlement calculator.
        let days = match category {
            Stock | Etf => [1, 1, 1, 1, 3, 2, 1].get(weekday).copied(),
            Bond => [2, 2, 2, 4, 4, 3, 2].get(weekday).copied(),
            Crypto | Fiat => Some(0),
            _ => None,
        }
        .expect("explicit weekday and category expectation");
        let expected = trade.created_at.date() + chrono::Duration::days(days);
        assert_eq!(trade.settlement_date, Some(expected));
        assert_eq!(
            WorkerTrade::read_trade(&mut connection, trade.id).unwrap(),
            trade
        );
    }
}
