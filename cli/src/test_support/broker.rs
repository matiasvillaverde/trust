//! Shared broker observations for dialog workflows. Unconfigured mutations fail.
//! Assertions belong to the facade/dialog scenarios, not to this double.
use model::{Account, Broker, BrokerKind, BrokerLog, Order, OrderIds, OrderStatus, Status, Trade};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Default)]
pub(crate) enum SyncBehavior {
    #[default]
    Unsupported,
    Filled,
    Terminal,
    SubmittedThenTerminal,
    Failing,
}

#[derive(Default)]
pub(crate) struct DialogBroker {
    pub(crate) sync: SyncBehavior,
    pub(crate) allow_close: bool,
    pub(crate) allow_modify: bool,
    pub(crate) sync_calls: Arc<AtomicUsize>,
}

impl Broker for DialogBroker {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Alpaca
    }

    fn submit_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog {
                trade_id: trade.id,
                log: "submitted".into(),
                ..BrokerLog::default()
            },
            OrderIds {
                entry: Uuid::new_v4().to_string(),
                stop: Uuid::new_v4().to_string(),
                target: Uuid::new_v4().to_string(),
            },
        ))
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        match self.sync {
            SyncBehavior::Unsupported => return Err("sync not supported".into()),
            SyncBehavior::Failing => return Err("sync failed".into()),
            _ => {}
        }
        let call = self.sync_calls.fetch_add(1, Ordering::SeqCst);
        if matches!(self.sync, SyncBehavior::SubmittedThenTerminal) && call == 0 {
            return Ok((Status::Submitted, vec![], BrokerLog::default()));
        }
        let mut entry = trade.entry.clone();
        entry.status = OrderStatus::Filled;
        entry.filled_quantity = entry.quantity;
        entry.average_filled_price = Some(dec!(100));
        entry.filled_at = Some(chrono::Utc::now().naive_utc());
        let mut target = trade.target.clone();
        let mut stop = trade.safety_stop.clone();
        let status = if matches!(self.sync, SyncBehavior::Filled) {
            target.status = OrderStatus::Accepted;
            stop.status = OrderStatus::Held;
            Status::Filled
        } else {
            target.status = OrderStatus::Filled;
            target.filled_quantity = target.quantity;
            target.average_filled_price = Some(dec!(110));
            target.filled_at = Some(chrono::Utc::now().naive_utc());
            stop.status = OrderStatus::Canceled;
            Status::ClosedTarget
        };
        Ok((
            status,
            vec![entry, target, stop],
            BrokerLog {
                trade_id: trade.id,
                ..BrokerLog::default()
            },
        ))
    }

    fn close_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        if !self.allow_close {
            return Err("unexpected close".into());
        }
        let mut target = trade.target.clone();
        target.broker_order_id = Some("manual-close-target".into());
        target.status = OrderStatus::Filled;
        target.filled_quantity = target.quantity;
        target.average_filled_price = Some(target.unit_price);
        target.filled_at = Some(chrono::Utc::now().naive_utc());
        Ok((
            target,
            BrokerLog {
                trade_id: trade.id,
                log: "closed".into(),
                ..BrokerLog::default()
            },
        ))
    }

    fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
        Err("unexpected cancellation".into())
    }

    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        if !self.allow_modify {
            return Err("unexpected stop modification".into());
        }
        Ok("modified-stop".into())
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        if !self.allow_modify {
            return Err("unexpected target modification".into());
        }
        Ok("modified-target".into())
    }
}
