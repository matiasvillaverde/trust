//! Observable history contracts through the facade and isolated real SQLite.
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use diesel::{Connection, RunQueryDsl};
use model::{
    history::{PageRequest, TradeHistoryFilter, TransactionHistoryFilter},
    Account, Currency, DatabaseFactory, DraftTrade, Environment, Status, TradeCategory,
    TradingVehicleCategory, TransactionCategory,
};
use rust_decimal_macros::dec;

struct History {
    trust: TrustFacade,
    db: SqliteDatabase,
    sql: diesel::SqliteConnection,
    account: Account,
    other: Account,
    _directory: tempfile::TempDir,
}
impl History {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.db");
        let path = path.to_str().unwrap();
        let mut trust = TrustFacade::new_with_brokers(Box::new(SqliteDatabase::new(path)), vec![]);
        let account = trust
            .create_account("history", "", Environment::Paper, dec!(0), dec!(0))
            .unwrap();
        let other = trust
            .create_account("other", "", Environment::Paper, dec!(0), dec!(0))
            .unwrap();
        let db = SqliteDatabase::new(path);
        let sql = diesel::SqliteConnection::establish(path).unwrap();
        Self {
            trust,
            db,
            sql,
            account,
            other,
            _directory: directory,
        }
    }
    fn trade(&mut self, other: bool, symbol: &str, status: Status) -> model::Trade {
        let instrument = self
            .trust
            .create_trading_vehicle(symbol, None, &TradingVehicleCategory::Stock, "ibkr")
            .unwrap();
        let trade = self
            .trust
            .create_trade(
                DraftTrade {
                    account: if other {
                        self.other.clone()
                    } else {
                        self.account.clone()
                    },
                    trading_vehicle: instrument,
                    quantity: dec!(1),
                    currency: Currency::USD,
                    category: TradeCategory::Long,
                    thesis: None,
                    sector: None,
                    asset_class: None,
                    context: None,
                },
                dec!(90),
                dec!(100),
                dec!(110),
            )
            .unwrap();
        self.db
            .trade_write()
            .update_trade_status(status, &trade)
            .unwrap()
    }
}

#[test]
fn trade_filters_count_only_matches_and_pages_have_stable_tie_breakers() {
    let mut h = History::new();
    let active = h.trade(false, "MATCH_1", Status::Submitted);
    h.trade(false, "MATCHX1", Status::Filled);
    h.trade(false, "CLOSED", Status::ClosedTarget);
    h.trade(true, "OTHER", Status::Submitted);
    diesel::sql_query("UPDATE trades SET created_at = '2026-10-01 12:00:00'")
        .execute(&mut h.sql)
        .unwrap();
    let filter = TradeHistoryFilter {
        symbol: Some("match_1".into()),
        active_only: true,
        ..Default::default()
    };
    let matching = h
        .trust
        .trade_history_page(h.account.id, &filter, PageRequest::new(1, 0).unwrap())
        .unwrap();
    assert_eq!(matching.total, 1);
    assert_eq!(matching.items[0].id, active.id);
    assert_eq!(matching.next_offset, None);
    let mut expected = h.trust.account_trades(h.account.id).unwrap();
    expected.sort_by_key(|trade| (trade.created_at, trade.id));
    let mut observed = vec![];
    for offset in 0..3 {
        let page = h
            .trust
            .trade_history_page(
                h.account.id,
                &TradeHistoryFilter::default(),
                PageRequest::new(1, offset).unwrap(),
            )
            .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(
            page.next_offset,
            if offset < 2 { Some(offset + 1) } else { None }
        );
        observed.extend(page.items.into_iter().map(|trade| trade.id));
    }
    assert_eq!(
        observed,
        expected.iter().map(|trade| trade.id).collect::<Vec<_>>()
    );
    let empty = h
        .trust
        .trade_history_page(
            h.account.id,
            &TradeHistoryFilter::default(),
            PageRequest::new(2, 99).unwrap(),
        )
        .unwrap();
    assert!(empty.items.is_empty());
    assert_eq!((empty.total, empty.next_offset), (3, None));
    let batch = h.trust.active_trade_batch(h.account.id, None, 1).unwrap();
    assert_eq!(batch.len(), 1);
    let next = h
        .trust
        .active_trade_batch(h.account.id, Some(batch[0].id), 1)
        .unwrap();
    assert_eq!(next.len(), 1);
    assert!(next[0].id > batch[0].id);
    assert!(h
        .trust
        .active_trade_batch(h.account.id, Some(next[0].id), 1)
        .unwrap()
        .is_empty());
}

#[test]
fn distribution_receipts_page_newest_first_with_account_scope_and_stable_ties() {
    let mut h = History::new();
    let timestamp = chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let mut expected = vec![];
    for (account, date) in [
        (h.account.id, timestamp),
        (h.account.id, timestamp),
        (h.account.id, timestamp + chrono::Duration::seconds(1)),
        (h.other.id, timestamp + chrono::Duration::seconds(2)),
    ] {
        let receipt =
            h.db.distribution_write()
                .create_history(
                    account,
                    None,
                    dec!(10),
                    date,
                    Some(dec!(4)),
                    Some(dec!(2)),
                    Some(dec!(4)),
                )
                .unwrap();
        if account == h.account.id {
            expected.push(receipt);
        }
    }
    expected.sort_by_key(|receipt| std::cmp::Reverse((receipt.distribution_date, receipt.id)));
    let mut observed = vec![];
    for offset in 0..3 {
        let page = h
            .trust
            .distribution_history_page(h.account.id, PageRequest::new(1, offset).unwrap())
            .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(
            page.next_offset,
            if offset < 2 { Some(offset + 1) } else { None }
        );
        observed.extend(page.items);
    }
    assert_eq!(observed, expected);
    let empty = h
        .trust
        .distribution_history_page(h.account.id, PageRequest::new(1, 3).unwrap())
        .unwrap();
    assert_eq!(empty.total, 3);
    assert!(empty.items.is_empty());
    assert_eq!(empty.next_offset, None);
}

#[test]
fn small_pages_do_not_decode_trades_outside_the_page_or_scope() {
    let mut h = History::new();
    let first = h.trade(false, "FIRST", Status::New);
    let corrupt = h.trade(false, "CORRUPT", Status::New);
    diesel::sql_query("UPDATE orders SET unit_price = 'invalid-decimal' WHERE id = ?")
        .bind::<diesel::sql_types::Text, _>(corrupt.entry.id.to_string())
        .execute(&mut h.sql)
        .unwrap();
    let page = h
        .trust
        .trade_history_page(
            h.account.id,
            &TradeHistoryFilter::default(),
            PageRequest::new(1, 0).unwrap(),
        )
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].id, first.id);
    assert_eq!(page.next_offset, Some(1));
    assert!(h
        .trust
        .trade_history_page(
            h.account.id,
            &TradeHistoryFilter::default(),
            PageRequest::new(1, 1).unwrap()
        )
        .is_err());
}

#[test]
fn transaction_time_filter_is_inclusive_and_excludes_other_accounts_and_taxes() {
    let mut h = History::new();
    let mut expected = vec![];
    for currency in [Currency::USD, Currency::EUR] {
        let (transaction, _) = h
            .trust
            .create_transaction(
                &h.account,
                &TransactionCategory::Deposit,
                dec!(12.345),
                &currency,
            )
            .unwrap();
        expected.push(transaction.id);
    }
    h.trust
        .create_transaction(
            &h.other,
            &TransactionCategory::Deposit,
            dec!(999),
            &Currency::USD,
        )
        .unwrap();
    h.db.transaction_write()
        .create_transaction(
            &h.account,
            dec!(-1),
            &Currency::USD,
            TransactionCategory::WithdrawalTax,
        )
        .unwrap();
    diesel::sql_query("UPDATE transactions SET created_at = '2026-10-01 12:00:00'")
        .execute(&mut h.sql)
        .unwrap();
    let timestamp = chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let filter = TransactionHistoryFilter {
        from: timestamp,
        to: timestamp,
    };
    let first = h
        .trust
        .transaction_history_page(h.account.id, &filter, PageRequest::new(1, 0).unwrap())
        .unwrap();
    let second = h
        .trust
        .transaction_history_page(h.account.id, &filter, PageRequest::new(1, 1).unwrap())
        .unwrap();
    expected.sort();
    assert_eq!(vec![first.items[0].id, second.items[0].id], expected);
    assert_eq!((first.total, first.next_offset), (2, Some(1)));
    assert_eq!((second.total, second.next_offset), (2, None));
    assert_eq!(second.items[0].amount, dec!(12.345));
    let empty = TransactionHistoryFilter {
        from: timestamp + chrono::Duration::seconds(1),
        to: timestamp + chrono::Duration::days(1),
    };
    assert_eq!(
        h.trust
            .transaction_history_page(h.account.id, &empty, PageRequest::new(1, 0).unwrap())
            .unwrap()
            .total,
        0
    );
}

#[test]
fn callers_cannot_request_unbounded_pages() {
    for (limit, offset) in [(0, 0), (501, 0), (u64::MAX, 0), (1, u64::MAX)] {
        assert!(PageRequest::new(limit, offset).is_err());
    }
}
