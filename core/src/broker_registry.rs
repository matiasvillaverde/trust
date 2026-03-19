use chrono::{DateTime, Utc};
use model::{
    Account, BarTimeframe, Broker, BrokerKind, BrokerLog, Execution, FeeActivity, MarketBar,
    MarketDataChannel, MarketDataStreamEvent, MarketQuote, MarketTradeTick, Order, OrderIds,
    Status, Trade,
};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::error::Error;

pub(crate) struct BrokerRegistry {
    brokers: HashMap<BrokerKind, Box<dyn Broker>>,
    fallback_kind: BrokerKind,
}

impl BrokerRegistry {
    pub(crate) fn from_single(broker: Box<dyn Broker>) -> Self {
        let fallback_kind = broker.kind();
        let mut brokers = HashMap::new();
        brokers.insert(fallback_kind, broker);
        Self {
            brokers,
            fallback_kind,
        }
    }

    pub(crate) fn from_many(brokers: Vec<Box<dyn Broker>>) -> Self {
        let mut brokers_by_kind = HashMap::new();
        let mut fallback_kind = BrokerKind::Alpaca;

        for (index, broker) in brokers.into_iter().enumerate() {
            let kind = broker.kind();
            if index == 0 {
                fallback_kind = kind;
            }
            brokers_by_kind.insert(kind, broker);
        }

        Self {
            brokers: brokers_by_kind,
            fallback_kind,
        }
    }

    fn broker_for_account(&self, account: &Account) -> Result<&dyn Broker, Box<dyn Error>> {
        self.brokers
            .get(&account.broker_kind)
            .map(Box::as_ref)
            .ok_or_else(|| {
                format!(
                    "broker '{}' is not configured for account '{}'",
                    account.broker_kind, account.name
                )
                .into()
            })
    }
}

impl std::fmt::Debug for BrokerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let configured: Vec<&'static str> = self.brokers.keys().map(BrokerKind::as_str).collect();
        f.debug_struct("BrokerRegistry")
            .field("configured", &configured)
            .field("fallback_kind", &self.fallback_kind)
            .finish()
    }
}

impl Broker for BrokerRegistry {
    fn kind(&self) -> BrokerKind {
        self.fallback_kind
    }

    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        self.broker_for_account(account)?
            .submit_trade(trade, account)
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        self.broker_for_account(account)?.sync_trade(trade, account)
    }

    fn close_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        self.broker_for_account(account)?
            .close_trade(trade, account)
    }

    fn cancel_trade(&self, trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
        self.broker_for_account(account)?
            .cancel_trade(trade, account)
    }

    fn modify_stop(
        &self,
        trade: &Trade,
        account: &Account,
        new_stop_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        self.broker_for_account(account)?
            .modify_stop(trade, account, new_stop_price)
    }

    fn modify_target(
        &self,
        trade: &Trade,
        account: &Account,
        new_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        self.broker_for_account(account)?
            .modify_target(trade, account, new_price)
    }

    fn get_bars(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        timeframe: BarTimeframe,
        account: &Account,
    ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
        self.broker_for_account(account)?
            .get_bars(symbol, start, end, timeframe, account)
    }

    fn get_latest_quote(
        &self,
        symbol: &str,
        account: &Account,
    ) -> Result<MarketQuote, Box<dyn Error>> {
        self.broker_for_account(account)?
            .get_latest_quote(symbol, account)
    }

    fn get_latest_trade(
        &self,
        symbol: &str,
        account: &Account,
    ) -> Result<MarketTradeTick, Box<dyn Error>> {
        self.broker_for_account(account)?
            .get_latest_trade(symbol, account)
    }

    fn stream_market_data(
        &self,
        symbols: &[String],
        channels: &[MarketDataChannel],
        max_events: usize,
        timeout_seconds: u64,
        account: &Account,
    ) -> Result<Vec<MarketDataStreamEvent>, Box<dyn Error>> {
        self.broker_for_account(account)?.stream_market_data(
            symbols,
            channels,
            max_events,
            timeout_seconds,
            account,
        )
    }

    fn fetch_executions(
        &self,
        trade: &Trade,
        account: &Account,
        after: Option<DateTime<Utc>>,
    ) -> Result<Vec<Execution>, Box<dyn Error>> {
        self.broker_for_account(account)?
            .fetch_executions(trade, account, after)
    }

    fn fetch_fee_activities(
        &self,
        trade: &Trade,
        account: &Account,
        after: Option<DateTime<Utc>>,
    ) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
        self.broker_for_account(account)?
            .fetch_fee_activities(trade, account, after)
    }
}

#[cfg(test)]
mod tests {
    use super::BrokerRegistry;
    use model::{
        Account, Broker, BrokerKind, BrokerLog, Execution, FeeActivity, MarketBar,
        MarketDataChannel, MarketDataStreamEvent, MarketQuote, MarketTradeTick, Order, OrderIds,
        Status, Trade,
    };
    use rust_decimal::Decimal;
    use std::error::Error;

    struct StubBroker {
        kind: BrokerKind,
    }

    impl StubBroker {
        fn new(kind: BrokerKind) -> Self {
            Self { kind }
        }
    }

    impl Broker for StubBroker {
        fn kind(&self) -> BrokerKind {
            self.kind
        }

        fn submit_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
            Ok((
                BrokerLog::default(),
                OrderIds {
                    stop: format!("{}-stop", self.kind),
                    entry: format!("{}-entry", self.kind),
                    target: format!("{}-target", self.kind),
                },
            ))
        }

        fn sync_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
            Ok((Status::Submitted, vec![], BrokerLog::default()))
        }

        fn close_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
            Ok((Order::default(), BrokerLog::default()))
        }

        fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
            Ok(())
        }

        fn modify_stop(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_stop_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Ok(format!("{}-stop", self.kind))
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Ok(format!("{}-target", self.kind))
        }

        fn get_bars(
            &self,
            _symbol: &str,
            _start: chrono::DateTime<chrono::Utc>,
            _end: chrono::DateTime<chrono::Utc>,
            _timeframe: model::BarTimeframe,
            _account: &Account,
        ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
            Ok(vec![])
        }

        fn get_latest_quote(
            &self,
            _symbol: &str,
            _account: &Account,
        ) -> Result<MarketQuote, Box<dyn Error>> {
            Err("not used".into())
        }

        fn get_latest_trade(
            &self,
            _symbol: &str,
            _account: &Account,
        ) -> Result<MarketTradeTick, Box<dyn Error>> {
            Err("not used".into())
        }

        fn stream_market_data(
            &self,
            _symbols: &[String],
            _channels: &[MarketDataChannel],
            _max_events: usize,
            _timeout_seconds: u64,
            _account: &Account,
        ) -> Result<Vec<MarketDataStreamEvent>, Box<dyn Error>> {
            Ok(vec![])
        }

        fn fetch_executions(
            &self,
            _trade: &Trade,
            _account: &Account,
            _after: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<Vec<Execution>, Box<dyn Error>> {
            Ok(vec![])
        }

        fn fetch_fee_activities(
            &self,
            _trade: &Trade,
            _account: &Account,
            _after: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
            Ok(vec![])
        }
    }

    #[test]
    fn registry_routes_submit_trade_by_account_broker_kind() {
        let registry = BrokerRegistry::from_many(vec![
            Box::new(StubBroker::new(BrokerKind::Alpaca)),
            Box::new(StubBroker::new(BrokerKind::Ibkr)),
        ]);

        let account = Account {
            broker_kind: BrokerKind::Ibkr,
            ..Account::default()
        };

        let (_, ids) = registry.submit_trade(&Trade::default(), &account).unwrap();
        assert_eq!(ids.entry, "ibkr-entry");
    }

    #[test]
    fn registry_returns_clear_error_when_account_broker_is_not_registered() {
        let registry = BrokerRegistry::from_single(Box::new(StubBroker::new(BrokerKind::Alpaca)));
        let account = Account {
            broker_kind: BrokerKind::Ibkr,
            name: "ibkr-main".to_string(),
            ..Account::default()
        };

        let err = registry
            .submit_trade(&Trade::default(), &account)
            .expect_err("missing broker should fail");
        assert!(err.to_string().contains("ibkr"));
        assert!(err.to_string().contains("ibkr-main"));
    }
}
