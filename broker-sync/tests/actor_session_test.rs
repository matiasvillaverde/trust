use broker_sync::{BrokerCommand, BrokerEvent, BrokerSync};
use std::time::Duration;
use uuid::Uuid;

#[test]
fn test_actor_session_start_list_stop() {
    let handle = BrokerSync::spawn();
    let account_id = Uuid::new_v4();
    let trade_id = Uuid::new_v4();

    handle
        .send(BrokerCommand::StartTradeSession {
            account_id,
            trade_id,
        })
        .expect("send start");

    let started = handle
        .recv_timeout(Duration::from_secs(1))
        .expect("started event");
    assert_eq!(
        started,
        BrokerEvent::TradeSessionStarted {
            account_id,
            trade_id
        }
    );

    handle
        .send(BrokerCommand::ListTradeSessions)
        .expect("list sessions");
    let snapshot = handle
        .recv_timeout(Duration::from_secs(1))
        .expect("snapshot event");
    match snapshot {
        BrokerEvent::TradeSessionSnapshot { sessions } => {
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].trade_id, trade_id);
            assert_eq!(sessions[0].account_id, account_id);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    handle
        .send(BrokerCommand::StopTradeSession { trade_id })
        .expect("stop session");
    let stopped = handle
        .recv_timeout(Duration::from_secs(1))
        .expect("stopped event");
    assert_eq!(
        stopped,
        BrokerEvent::TradeSessionStopped {
            trade_id,
            reason: "stopped".to_string(),
        }
    );

    handle.send(BrokerCommand::Shutdown).expect("shutdown");
}
