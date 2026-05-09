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

#[test]
fn test_actor_status_and_touch_session_paths() {
    let handle = BrokerSync::spawn();
    let account_id = Uuid::new_v4();
    let trade_id = Uuid::new_v4();

    handle
        .send(BrokerCommand::StartTradeSession {
            account_id,
            trade_id,
        })
        .expect("send start");
    assert_eq!(
        handle.recv().expect("started event"),
        BrokerEvent::TradeSessionStarted {
            account_id,
            trade_id,
        }
    );

    handle
        .send(BrokerCommand::StartSync { account_id })
        .expect("start sync compatibility command");
    handle
        .send(BrokerCommand::StopSync { account_id })
        .expect("stop sync compatibility command");
    handle
        .send(BrokerCommand::ManualReconcile {
            account_id: Some(account_id),
            force: true,
        })
        .expect("manual reconcile compatibility command");
    assert!(
        handle.recv_timeout(Duration::from_millis(25)).is_err(),
        "compatibility-only commands should not emit actor events"
    );

    handle
        .send(BrokerCommand::ListTradeSessions)
        .expect("list sessions before touch");
    let before = handle
        .recv_timeout(Duration::from_secs(1))
        .expect("snapshot before touch");
    let before_last_activity = match before {
        BrokerEvent::TradeSessionSnapshot { sessions } => {
            assert_eq!(sessions.len(), 1);
            sessions[0].last_activity_at_ms
        }
        other => panic!("unexpected event: {other:?}"),
    };

    std::thread::sleep(Duration::from_millis(2));
    handle
        .send(BrokerCommand::TouchTradeSession { trade_id })
        .expect("touch session");
    handle
        .send(BrokerCommand::TouchTradeSession {
            trade_id: Uuid::new_v4(),
        })
        .expect("touch missing session is ignored");
    handle
        .send(BrokerCommand::ListTradeSessions)
        .expect("list sessions after touch");
    let after = handle
        .recv_timeout(Duration::from_secs(1))
        .expect("snapshot after touch");
    match after {
        BrokerEvent::TradeSessionSnapshot { sessions } => {
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].trade_id, trade_id);
            assert!(
                sessions[0].last_activity_at_ms >= before_last_activity,
                "touch should not move activity timestamp backward"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }

    handle.send(BrokerCommand::GetStatus).expect("get status");
    assert_eq!(handle.recv().expect("status event"), BrokerEvent::GetStatus);

    handle.send(BrokerCommand::Shutdown).expect("shutdown");
}
