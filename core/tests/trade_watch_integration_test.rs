use chrono::{DateTime, Utc};
use db_sqlite::SqliteDatabase;
use model::{
    Account, BarTimeframe, Broker, BrokerLog, MarketBar, OrderIds, Status, Trade, TradeCategory,
    TradingVehicleCategory, WatchControl, WatchEvent, WatchOptions,
};
use rust_decimal_macros::dec;
use std::error::Error;
use std::time::Duration;
use uuid::Uuid;

struct MockBroker {
    submit_ids: OrderIds,
}

impl Default for MockBroker {
    fn default() -> Self {
        Self {
            submit_ids: OrderIds {
                entry: Uuid::new_v4(),
                stop: Uuid::new_v4(),
                target: Uuid::new_v4(),
            },
        }
    }
}

impl Broker for MockBroker {
    fn submit_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        let now = chrono::Utc::now().naive_utc();
        let log = BrokerLog {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: trade.id,
            log: "{\"submit\":\"ok\"}".to_string(),
        };

        Ok((
            log,
            OrderIds {
                entry: self.submit_ids.entry,
                stop: self.submit_ids.stop,
                target: self.submit_ids.target,
            },
        ))
    }

    fn sync_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<model::Order>, BrokerLog), Box<dyn Error>> {
        Err("not used in this test".into())
    }

    fn close_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(model::Order, BrokerLog), Box<dyn Error>> {
        Err("not used in this test".into())
    }

    fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
        Err("not used in this test".into())
    }

    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_stop_price: rust_decimal::Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        Err("not used in this test".into())
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_price: rust_decimal::Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        Err("not used in this test".into())
    }

    fn get_bars(
        &self,
        _symbol: &str,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
        _timeframe: BarTimeframe,
        _account: &Account,
    ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
        Err("not used in this test".into())
    }

    fn watch_trade(
        &self,
        trade: &Trade,
        _account: &Account,
        _options: WatchOptions,
        on_event: &mut dyn FnMut(WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
    ) -> Result<(), Box<dyn Error>> {
        let invalid = WatchEvent {
            broker_source: "alpaca".to_string(),
            broker_stream: "trade_updates".to_string(),
            updated_orders: Vec::new(),
            message: None,
            broker_event_type: "test_invalid_payload".to_string(),
            broker_order_id: None,
            market_price: None,
            market_timestamp: None,
            market_symbol: None,
            payload_json: "{not-json".to_string(),
        };
        let _ = on_event(invalid)?;

        let mut filled_entry = trade.entry.clone();
        filled_entry.status = model::OrderStatus::Filled;
        filled_entry.filled_quantity = trade.entry.quantity;
        filled_entry.average_filled_price = Some(dec!(100));

        let fill = WatchEvent {
            broker_source: "alpaca".to_string(),
            broker_stream: "trade_updates".to_string(),
            updated_orders: vec![filled_entry.clone()],
            message: None,
            broker_event_type: "fill".to_string(),
            broker_order_id: filled_entry.broker_order_id,
            market_price: None,
            market_timestamp: None,
            market_symbol: None,
            payload_json: "{\"event\":\"fill\"}".to_string(),
        };
        let _ = on_event(fill)?;

        Ok(())
    }
}

#[test]
fn watch_trade_persists_events_and_updates_trade_status() {
    let database = SqliteDatabase::new_in_memory();

    let broker = MockBroker {
        submit_ids: OrderIds {
            entry: Uuid::new_v4(),
            stop: Uuid::new_v4(),
            target: Uuid::new_v4(),
        },
    };

    let mut trust = core::TrustFacade::new(Box::new(database), Box::new(broker));

    let account = trust
        .create_account("t", "t", model::Environment::Paper, dec!(0), dec!(0))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(100000),
            &model::Currency::USD,
        )
        .unwrap();

    let vehicle = trust
        .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
        .unwrap();

    let draft = model::DraftTrade {
        account: account.clone(),
        trading_vehicle: vehicle,
        quantity: 1,
        currency: model::Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };

    let trade = trust
        .create_trade(draft, dec!(90), dec!(100), dec!(110))
        .unwrap();

    let (_trade_ignored, _, _, _) = trust.fund_trade(&trade).unwrap();
    let trade = trust.read_trade(trade.id).unwrap();
    assert_eq!(trade.status, Status::Funded);

    let (trade, _log) = trust.submit_trade(&trade).unwrap();
    assert_eq!(trade.status, Status::Submitted);

    trust
        .watch_trade(
            &trade,
            &account,
            WatchOptions {
                reconcile_every: Duration::from_secs(60),
                timeout: Some(Duration::from_secs(2)),
            },
            |_trade_now, _evt| Ok(WatchControl::Continue),
        )
        .unwrap();

    let trade_latest = trust.read_trade(trade.id).unwrap();
    assert_eq!(trade_latest.status, Status::Filled);

    let events = trust.read_broker_events_for_trade(trade.id).unwrap();
    assert!(events.len() >= 2);
    assert_eq!(events[0].source, "alpaca");
    assert_eq!(events[0].stream, "trade_updates");
    assert_eq!(events[0].event_type, "test_invalid_payload");
    assert_eq!(events[0].payload_json, "{\"error\":\"payload_invalid_json\"}");
}
