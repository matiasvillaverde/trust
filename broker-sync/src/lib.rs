//! In-memory trade session tracking for the legacy CLI.
//!
//! This actor does not connect to a broker or reconcile orders. The exported
//! connection state machine is a separate, pure model. Broker reconciliation
//! belongs to TrustFacade and the durable service operation journal.

mod messages;
mod state;

// Re-export public types
pub use messages::{
    BrokerCommand, BrokerEvent, OrderUpdate, ReconciliationStatus, TradeSessionStatus,
};
pub use state::{BackoffConfig, BrokerState, StateError, StateTransition};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TradeSession {
    account_id: Uuid,
    trade_id: Uuid,
    started_at_ms: i64,
    last_activity_at_ms: i64,
}

impl TradeSession {
    fn new(account_id: Uuid, trade_id: Uuid, now_ms: i64) -> Self {
        Self {
            account_id,
            trade_id,
            started_at_ms: now_ms,
            last_activity_at_ms: now_ms,
        }
    }

    fn snapshot(&self) -> TradeSessionStatus {
        TradeSessionStatus {
            account_id: self.account_id,
            trade_id: self.trade_id,
            started_at_ms: self.started_at_ms,
            last_activity_at_ms: self.last_activity_at_ms,
        }
    }
}

fn now_ms() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(now.as_millis()).unwrap_or(i64::MAX)
}

/// Handle to the background trade session tracker.
#[derive(Debug)]
pub struct TradeSessionHandle {
    sender: Sender<BrokerCommand>,
    events: Receiver<BrokerEvent>,
}

impl TradeSessionHandle {
    /// Send a command to the actor.
    pub fn send(&self, command: BrokerCommand) -> Result<(), String> {
        self.sender.send(command).map_err(|e| e.to_string())
    }

    /// Receive one actor event (blocking).
    pub fn recv(&self) -> Result<BrokerEvent, String> {
        self.events.recv().map_err(|e| e.to_string())
    }

    /// Receive one actor event with timeout.
    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<BrokerEvent, std::sync::mpsc::RecvTimeoutError> {
        self.events.recv_timeout(timeout)
    }
}

/// In-memory session tracker; no broker I/O is performed.
pub struct TradeSessionTracker;

impl TradeSessionTracker {
    /// Spawn the actor in a background thread.
    pub fn spawn() -> TradeSessionHandle {
        let (cmd_tx, cmd_rx): (Sender<BrokerCommand>, Receiver<BrokerCommand>) = mpsc::channel();
        let (evt_tx, evt_rx): (Sender<BrokerEvent>, Receiver<BrokerEvent>) = mpsc::channel();

        thread::spawn(move || {
            let mut sessions: HashMap<Uuid, TradeSession> = HashMap::new();
            while let Ok(command) = cmd_rx.recv() {
                match command {
                    BrokerCommand::StartTradeSession {
                        account_id,
                        trade_id,
                    } => {
                        let now = now_ms();
                        sessions.insert(trade_id, TradeSession::new(account_id, trade_id, now));
                        let _ = evt_tx.send(BrokerEvent::TradeSessionStarted {
                            account_id,
                            trade_id,
                        });
                    }
                    BrokerCommand::StopTradeSession { trade_id } => {
                        sessions.remove(&trade_id);
                        let _ = evt_tx.send(BrokerEvent::TradeSessionStopped {
                            trade_id,
                            reason: "stopped".to_string(),
                        });
                    }
                    BrokerCommand::TouchTradeSession { trade_id } => {
                        if let Some(session) = sessions.get_mut(&trade_id) {
                            session.last_activity_at_ms = now_ms();
                        }
                    }
                    BrokerCommand::ListTradeSessions => {
                        let snapshot: Vec<TradeSessionStatus> =
                            sessions.values().map(TradeSession::snapshot).collect();
                        let _ =
                            evt_tx.send(BrokerEvent::TradeSessionSnapshot { sessions: snapshot });
                    }
                    BrokerCommand::Shutdown => {
                        break;
                    }
                    BrokerCommand::GetStatus => {
                        let _ = evt_tx.send(BrokerEvent::GetStatus);
                    }
                }
            }
        });

        TradeSessionHandle {
            sender: cmd_tx,
            events: evt_rx,
        }
    }
}
