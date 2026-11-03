//! Recovery of broker evidence independently of a trade's lifecycle status.
use crate::{
    audit, commands, synced_close_reason, TrustFacade, CLOSE_PROCESSING_COMPLETE_LOG,
    CLOSE_PROCESSING_SAVEPOINT,
};
use chrono::{DateTime, Duration, Utc};
use model::{Account, BrokerLog, Order, Status, Trade};
use std::{error::Error, fmt};

/// Known order updates were retained, but one or more broker activity reads failed.
#[derive(Debug)]
pub struct IncompleteEvidence;

impl fmt::Display for IncompleteEvidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Broker execution or fee evidence is incomplete; confirmed order updates were retained. Reconcile again before resuming new risk.")
    }
}
impl Error for IncompleteEvidence {}

impl TrustFacade {
    /// Continue collecting late activity for terminal broker trades over IBKR's
    /// seven-day activity window. An explicitly incomplete sync remains eligible
    /// after that window; recovery must not silently discard a known failure.
    pub fn terminal_broker_reconciliation_due(
        &mut self,
        trade: &Trade,
        now: DateTime<Utc>,
    ) -> Result<bool, Box<dyn Error>> {
        if !matches!(
            trade.status,
            Status::ClosedTarget
                | Status::ClosedStopLoss
                | Status::Canceled
                | Status::Expired
                | Status::Rejected
        ) || trade.entry.broker_order_id.is_none()
        {
            return Ok(false);
        }
        let last_activity = [&trade.entry, &trade.target, &trade.safety_stop]
            .into_iter()
            .flat_map(|order| [order.filled_at, order.cancelled_at, order.expired_at])
            .flatten()
            .max();
        if last_activity
            .is_none_or(|time| now.naive_utc().signed_duration_since(time) <= Duration::days(7))
        {
            return Ok(true);
        }
        self.broker_reconciliation_incomplete(trade.id)
    }

    /// Whether the latest durable broker sync retained an activity-read failure.
    pub fn broker_reconciliation_incomplete(
        &mut self,
        trade_id: uuid::Uuid,
    ) -> Result<bool, Box<dyn Error>> {
        let logs = self.factory.log_read().read_all_logs_for_trade(trade_id)?;
        Ok(audit::ordered_events(&logs)?
            .iter()
            .rev()
            .find(|event| event.event_type == "trade.sync")
            .is_some_and(|event| event.outcome == "incomplete"))
    }
}

impl TrustFacade {
    /// Synchronize trade status with the broker.
    ///
    /// # Arguments
    ///
    /// * `trade` - The trade to synchronize
    /// * `account` - The account associated with the trade
    ///
    /// # Returns
    ///
    /// Returns a tuple of the updated status, orders, and broker log.
    pub fn sync_trade(
        &mut self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
        let (status, orders, log, transitioned_to_closed, persisted_trade) =
            commands::trade::sync_with_broker(
                trade,
                account,
                &mut *self.factory,
                &mut *self.broker,
            )?;

        let is_closed = status == Status::ClosedTarget || status == Status::ClosedStopLoss;
        let processed_close = if is_closed {
            self.financial_transaction(CLOSE_PROCESSING_SAVEPOINT, |facade| {
                // Claim post-close work under the same writer reservation as its marker.
                let processing_complete = facade
                    .factory
                    .log_read()
                    .read_all_logs_for_trade(persisted_trade.id)?
                    .iter()
                    .any(|entry| entry.log == CLOSE_PROCESSING_COMPLETE_LOG);
                if !processing_complete {
                    let close_reason = synced_close_reason(status, persisted_trade.target.category);
                    facade.handle_trade_closed_event_from_trade(&persisted_trade, close_reason)?;
                    facade
                        .factory
                        .log_write()
                        .create_log(CLOSE_PROCESSING_COMPLETE_LOG, &persisted_trade)?;
                }
                Ok(!processing_complete)
            })?
        } else {
            false
        };

        if transitioned_to_closed || processed_close {
            // Recovery can complete post-close work after an earlier incomplete sync.
            // Auto-grading on close is enabled by default; we keep it best-effort for sync reliability.
            let has_grade = match self
                .factory
                .trade_grade_read()
                .read_latest_for_trade(trade.id)
            {
                Ok(opt) => opt.is_some(),
                Err(_) => true, // Can't read grades; treat as "don't try" to keep sync reliable.
            };

            if !has_grade {
                let mut grader = crate::services::grading::TradeGradeService::new(
                    &mut *self.factory,
                    &mut *self.broker,
                );
                let _ = grader.grade_trade(
                    trade.id,
                    crate::services::grading::GradingWeightsPermille::default(),
                );
            }
        }

        Ok((status, orders, log))
    }
}
