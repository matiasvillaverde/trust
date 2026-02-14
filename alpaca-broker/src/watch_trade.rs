use crate::keys;
use crate::order_mapper;
use apca::api::v2::order::{Get, Id, Order as AlpacaOrder};
use apca::api::v2::updates;
use apca::data::v2::stream::{IEX, MarketData, RealtimeData};
use apca::Client;
use apca::Subscribable;
use broker_sync::{BrokerState, StateTransition};
use futures_util::StreamExt as _;
use model::{Account, Order, OrderStatus, Trade, WatchControl, WatchEvent, WatchOptions};
use std::error::Error;
use std::str::FromStr;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::time;
use uuid::Uuid;

/// Watch a trade using Alpaca `trade_updates` websocket, with periodic REST reconciliation.
///
/// This function is synchronous (blocks) and internally drives a Tokio runtime.
pub fn watch(
    trade: &Trade,
    account: &Account,
    options: WatchOptions,
    on_event: &mut dyn FnMut(WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    assert!(trade.account_id == account.id); // Verify trade belongs to account

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(watch_async(trade, client, options, on_event))
}

fn event_type_str(event: updates::OrderStatus) -> &'static str {
    match event {
        updates::OrderStatus::New => "new",
        updates::OrderStatus::Replaced => "replaced",
        updates::OrderStatus::ReplaceRejected => "order_replace_rejected",
        updates::OrderStatus::PartialFill => "partial_fill",
        updates::OrderStatus::Filled => "fill",
        updates::OrderStatus::DoneForDay => "done_for_day",
        updates::OrderStatus::Canceled => "canceled",
        updates::OrderStatus::CancelRejected => "order_cancel_rejected",
        updates::OrderStatus::Expired => "expired",
        updates::OrderStatus::PendingCancel => "pending_cancel",
        updates::OrderStatus::Stopped => "stopped",
        updates::OrderStatus::Rejected => "rejected",
        updates::OrderStatus::Suspended => "suspended",
        updates::OrderStatus::PendingNew => "pending_new",
        updates::OrderStatus::PendingReplace => "pending_replace",
        updates::OrderStatus::Calculated => "calculated",
        updates::OrderStatus::Unknown => "unknown",
        // apca marks this enum as non-exhaustive.
        _ => "unknown",
    }
}

fn is_terminal(entry: &Order, stop: &Order, target: &Order) -> bool {
    // Exit orders filled => terminal.
    if stop.status == OrderStatus::Filled || target.status == OrderStatus::Filled {
        return true;
    }

    // Entry terminal without being filled => terminal (canceled/expired/rejected).
    matches!(
        entry.status,
        OrderStatus::Canceled | OrderStatus::Expired | OrderStatus::Rejected
    )
}

async fn reconcile_once(
    client: &Client,
    entry: &Order,
    stop: &Order,
    target: &Order,
) -> Result<Vec<(Uuid, AlpacaOrder)>, Box<dyn Error>> {
    let mut result = Vec::new();

    let entry_id = entry
        .broker_order_id
        .ok_or("Entry broker_order_id missing; cannot reconcile")?;
    let stop_id = stop
        .broker_order_id
        .ok_or("Stop broker_order_id missing; cannot reconcile")?;
    let target_id = target
        .broker_order_id
        .ok_or("Target broker_order_id missing; cannot reconcile")?;

    let entry_order = client.issue::<Get>(&Id(entry_id)).await?;
    result.push((entry_id, entry_order));

    let stop_order = client.issue::<Get>(&Id(stop_id)).await?;
    result.push((stop_id, stop_order));

    let target_order = client.issue::<Get>(&Id(target_id)).await?;
    result.push((target_id, target_order));

    Ok(result)
}

async fn watch_async(
    trade: &Trade,
    client: Client,
    options: WatchOptions,
    on_event: &mut dyn FnMut(WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    // Local mutable copies to avoid emitting duplicate updates for the same state.
    let mut entry = trade.entry.clone();
    let mut stop = trade.safety_stop.clone();
    let mut target = trade.target.clone();

    // Basic sanity: to watch we need broker ids for the 3 orders.
    if entry.broker_order_id.is_none() || stop.broker_order_id.is_none() || target.broker_order_id.is_none()
    {
        return Err(
            "Trade orders are missing broker_order_id; submit the trade before watching".into(),
        );
    }

    let symbol_expected = trade.trading_vehicle.symbol.to_uppercase();

    let start = Instant::now();

    // Setup periodic reconciliation (REST).
    let mut reconcile_tick = time::interval(options.reconcile_every);
    reconcile_tick.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

    // Note: we intentionally keep subscriptions alive; dropping them may close the socket.
    type OrderUpdatesStream = <updates::OrderUpdates as Subscribable>::Stream;
    type OrderUpdatesSub = <updates::OrderUpdates as Subscribable>::Subscription;
    let mut ws_state = BrokerState::Disconnected;
    let mut ws_stream: Option<std::pin::Pin<Box<OrderUpdatesStream>>> = None;
    let mut _ws_sub: Option<OrderUpdatesSub> = None;

    type MarketDataStream = <RealtimeData<IEX> as Subscribable>::Stream;
    type MarketDataSub = <RealtimeData<IEX> as Subscribable>::Subscription;
    let mut md_state = BrokerState::Disconnected;
    let mut md_stream: Option<std::pin::Pin<Box<MarketDataStream>>> = None;
    let mut _md_sub: Option<MarketDataSub> = None;

    let mut market_data_req = MarketData::default();
    market_data_req.set_trades(vec![symbol_expected.clone()]);

    // Emit a best-effort initial reconciliation snapshot so we start from server truth.
    // (We do this outside the main loop so the CLI gets an immediate snapshot.)
    if let Ok(reconciled) = reconcile_once(&client, &entry, &stop, &target).await {
        let mut updated_orders = Vec::new();
        for (id, alpaca_order) in reconciled {
            if alpaca_order.symbol.to_uppercase() != symbol_expected {
                continue;
            }

            if Some(id) == entry.broker_order_id {
                let mapped = order_mapper::map_single(&alpaca_order, &entry)?;
                if mapped != entry {
                    entry = mapped.clone();
                    updated_orders.push(mapped);
                }
            } else if Some(id) == stop.broker_order_id {
                let mapped = order_mapper::map_single(&alpaca_order, &stop)?;
                if mapped != stop {
                    stop = mapped.clone();
                    updated_orders.push(mapped);
                }
            } else if Some(id) == target.broker_order_id {
                let mapped = order_mapper::map_single(&alpaca_order, &target)?;
                if mapped != target {
                    target = mapped.clone();
                    updated_orders.push(mapped);
                }
            }
        }

        if !updated_orders.is_empty() {
            let evt = WatchEvent {
                broker_source: "alpaca".to_string(),
                broker_stream: "trading_rest".to_string(),
                updated_orders,
                message: Some("initial_reconcile".to_string()),
                broker_event_type: "reconcile".to_string(),
                broker_order_id: None,
                market_price: None,
                market_timestamp: None,
                market_symbol: None,
                payload_json: "{}".to_string(),
            };
            if matches!(on_event(evt)?, WatchControl::Stop) {
                return Ok(());
            }
        }
    }

    loop {
        if let Some(timeout) = options.timeout {
            if start.elapsed() > timeout {
                return Ok(());
            }
        }

        if is_terminal(&entry, &stop, &target) {
            return Ok(());
        }

        let now = Instant::now();

        // Connect (or reconnect) the order-updates websocket.
        if ws_stream.is_none() {
            let can_try = match &ws_state {
                BrokerState::ErrorRecovery { next_retry, .. } => now >= *next_retry,
                _ => true,
            };
            if can_try {
                ws_state = ws_state
                    .clone()
                    .transition_at(StateTransition::Connect, now)
                    .unwrap_or(BrokerState::Connecting);

                match client.subscribe::<updates::OrderUpdates>().await {
                    Ok((stream, sub)) => {
                        ws_stream = Some(Box::pin(stream));
                        _ws_sub = Some(sub);
                        ws_state = ws_state
                            .clone()
                            .transition_at(StateTransition::ConnectionEstablished, now)
                            .unwrap_or(BrokerState::Reconciling { start_time: now });

                        // After connecting, force a reconciliation so we "heal" any missed messages.
                        if let Ok(reconciled) = reconcile_once(&client, &entry, &stop, &target).await {
                            let mut updated_orders = Vec::new();
                            for (id, alpaca_order) in reconciled {
                                if alpaca_order.symbol.to_uppercase() != symbol_expected {
                                    continue;
                                }

                                if Some(id) == entry.broker_order_id {
                                    let mapped = order_mapper::map_single(&alpaca_order, &entry)?;
                                    if mapped != entry {
                                        entry = mapped.clone();
                                        updated_orders.push(mapped);
                                    }
                                } else if Some(id) == stop.broker_order_id {
                                    let mapped = order_mapper::map_single(&alpaca_order, &stop)?;
                                    if mapped != stop {
                                        stop = mapped.clone();
                                        updated_orders.push(mapped);
                                    }
                                } else if Some(id) == target.broker_order_id {
                                    let mapped = order_mapper::map_single(&alpaca_order, &target)?;
                                    if mapped != target {
                                        target = mapped.clone();
                                        updated_orders.push(mapped);
                                    }
                                }
                            }

                            if !updated_orders.is_empty() {
                                let evt = WatchEvent {
                                    broker_source: "alpaca".to_string(),
                                    broker_stream: "trading_rest".to_string(),
                                    updated_orders,
                                    message: Some("ws_connected_reconcile".to_string()),
                                    broker_event_type: "reconcile".to_string(),
                                    broker_order_id: None,
                                    market_price: None,
                                    market_timestamp: None,
                                    market_symbol: None,
                                    payload_json: "{}".to_string(),
                                };
                                if matches!(on_event(evt)?, WatchControl::Stop) {
                                    return Ok(());
                                }
                            }
                        }

                        ws_state = ws_state
                            .clone()
                            .transition_at(StateTransition::ReconciliationComplete, now)
                            .unwrap_or(BrokerState::Live {
                                connected_since: now,
                            });
                    }
                    Err(_err) => {
                        ws_stream = None;
                        _ws_sub = None;
                        ws_state = ws_state
                            .clone()
                            .transition_at(StateTransition::Error, now)
                            .unwrap_or_else(|_| BrokerState::Disconnected);
                    }
                }
            }
        }

        // Connect (or reconnect) the market-data websocket. Failures here should not stop order watching.
        if md_stream.is_none() {
            let can_try = match &md_state {
                BrokerState::ErrorRecovery { next_retry, .. } => now >= *next_retry,
                _ => true,
            };
            if can_try {
                md_state = md_state
                    .clone()
                    .transition_at(StateTransition::Connect, now)
                    .unwrap_or(BrokerState::Connecting);

                match client.subscribe::<RealtimeData<IEX>>().await {
                    Ok((stream, mut sub)) => {
                        // Subscribe to trades for our symbol; if this fails, treat as transient.
                        let subscribe_result = sub.subscribe(&market_data_req).await;
                        match subscribe_result {
                            Ok(Ok(())) => {
                                md_stream = Some(Box::pin(stream));
                                _md_sub = Some(sub);
                                md_state = BrokerState::Live {
                                    connected_since: now,
                                };
                            }
                            _ => {
                                md_stream = None;
                                _md_sub = None;
                                md_state = md_state
                                    .clone()
                                    .transition_at(StateTransition::Error, now)
                                    .unwrap_or_else(|_| BrokerState::Disconnected);
                            }
                        }
                    }
                    Err(_err) => {
                        md_stream = None;
                        _md_sub = None;
                        md_state = md_state
                            .clone()
                            .transition_at(StateTransition::Error, now)
                            .unwrap_or_else(|_| BrokerState::Disconnected);
                    }
                }
            }
        }

        let ws_backoff = ws_state.backoff_duration();
        let md_backoff = md_state.backoff_duration();
        let idle_sleep = ws_backoff
            .min(md_backoff)
            .max(std::time::Duration::from_millis(100));

        tokio::select! {
            _ = reconcile_tick.tick() => {
                match reconcile_once(&client, &entry, &stop, &target).await {
                    Ok(reconciled) => {
                        let mut updated_orders = Vec::new();
                        for (id, alpaca_order) in reconciled {
                            if alpaca_order.symbol.to_uppercase() != symbol_expected {
                                continue;
                            }

                            if Some(id) == entry.broker_order_id {
                                let mapped = order_mapper::map_single(&alpaca_order, &entry)?;
                                if mapped != entry {
                                    entry = mapped.clone();
                                    updated_orders.push(mapped);
                                }
                            } else if Some(id) == stop.broker_order_id {
                                let mapped = order_mapper::map_single(&alpaca_order, &stop)?;
                                if mapped != stop {
                                    stop = mapped.clone();
                                    updated_orders.push(mapped);
                                }
                            } else if Some(id) == target.broker_order_id {
                                let mapped = order_mapper::map_single(&alpaca_order, &target)?;
                                if mapped != target {
                                    target = mapped.clone();
                                    updated_orders.push(mapped);
                                }
                            }
                        }

                        if !updated_orders.is_empty() {
                            let evt = WatchEvent {
                                broker_source: "alpaca".to_string(),
                                broker_stream: "trading_rest".to_string(),
                                updated_orders,
                                message: Some("reconcile".to_string()),
                                broker_event_type: "reconcile".to_string(),
                                broker_order_id: None,
                                market_price: None,
                                market_timestamp: None,
                                market_symbol: None,
                                payload_json: "{}".to_string(),
                            };
                            if matches!(on_event(evt)?, WatchControl::Stop) {
                                return Ok(());
                            }
                        }
                    }
                    Err(_err) => {
                        // Keep running; reconciliation errors are treated as transient.
                    }
                }
            }

            // Prevent a busy loop while we're in error recovery and neither websocket is connected.
            _ = time::sleep(idle_sleep) , if ws_stream.is_none() && md_stream.is_none() && (ws_backoff > std::time::Duration::from_secs(0) || md_backoff > std::time::Duration::from_secs(0)) => {}

            maybe = async {
                match ws_stream.as_mut() {
                    Some(stream) => stream.as_mut().next().await,
                    None => std::future::pending().await,
                }
            } => {
                let msg = match maybe {
                    Some(m) => m,
                    None => {
                        ws_stream = None;
                        _ws_sub = None;
                        ws_state = ws_state
                            .clone()
                            .transition_at(StateTransition::Error, Instant::now())
                            .unwrap_or_else(|_| BrokerState::Disconnected);
                        continue;
                    }
                };

                let update = match msg {
                    Ok(Ok(update)) => update,
                    Ok(Err(_json_err)) => continue,
                    Err(_ws_err) => {
                        ws_stream = None;
                        _ws_sub = None;
                        ws_state = ws_state
                            .clone()
                            .transition_at(StateTransition::Error, Instant::now())
                            .unwrap_or_else(|_| BrokerState::Disconnected);
                        continue;
                    }
                };

                // Security: filter to only our symbol.
                if update.order.symbol.to_uppercase() != symbol_expected {
                    continue;
                }

                let event_type = event_type_str(update.event).to_string();
                let broker_order_id = Some(update.order.id.0);
                let payload_json = serde_json::to_string(&update).unwrap_or_else(|_| "{}".to_string());

                let mut updated_orders = Vec::new();
                if broker_order_id == entry.broker_order_id {
                    let mapped = order_mapper::map_single(&update.order, &entry)?;
                    if mapped != entry {
                        entry = mapped.clone();
                        updated_orders.push(mapped);
                    }
                } else if broker_order_id == stop.broker_order_id {
                    let mapped = order_mapper::map_single(&update.order, &stop)?;
                    if mapped != stop {
                        stop = mapped.clone();
                        updated_orders.push(mapped);
                    }
                } else if broker_order_id == target.broker_order_id {
                    let mapped = order_mapper::map_single(&update.order, &target)?;
                    if mapped != target {
                        target = mapped.clone();
                        updated_orders.push(mapped);
                    }
                } else {
                    continue;
                }

                if !updated_orders.is_empty() {
                    let evt = WatchEvent {
                        broker_source: "alpaca".to_string(),
                        broker_stream: "trade_updates".to_string(),
                        updated_orders,
                        message: None,
                        broker_event_type: event_type,
                        broker_order_id,
                        market_price: None,
                        market_timestamp: None,
                        market_symbol: None,
                        payload_json,
                    };
                    if matches!(on_event(evt)?, WatchControl::Stop) {
                        return Ok(());
                    }
                }
            }

            maybe = async {
                match md_stream.as_mut() {
                    Some(stream) => stream.as_mut().next().await,
                    None => std::future::pending().await,
                }
            } => {
                let msg = match maybe {
                    Some(m) => m,
                    None => {
                        md_stream = None;
                        _md_sub = None;
                        md_state = md_state
                            .clone()
                            .transition_at(StateTransition::Error, Instant::now())
                            .unwrap_or_else(|_| BrokerState::Disconnected);
                        continue;
                    }
                };

                let data = match msg {
                    Ok(Ok(data)) => data,
                    Ok(Err(_json_err)) => continue,
                    Err(_ws_err) => {
                        md_stream = None;
                        _md_sub = None;
                        md_state = md_state
                            .clone()
                            .transition_at(StateTransition::Error, Instant::now())
                            .unwrap_or_else(|_| BrokerState::Disconnected);
                        continue;
                    }
                };

                // Only use trade ticks for a "last price" display.
                if let apca::data::v2::stream::Data::Trade(trade_tick) = data {
                    let price =
                        rust_decimal::Decimal::from_str(&trade_tick.trade_price.to_string()).ok();
                    let payload_json = serde_json::to_string(&serde_json::json!({
                        "t": trade_tick.timestamp.to_rfc3339(),
                        "p": trade_tick.trade_price.to_string(),
                        "s": trade_tick.trade_size.to_string(),
                        "symbol": symbol_expected.clone(),
                    })).unwrap_or_else(|_| "{}".to_string());

                    let evt = WatchEvent {
                        broker_source: "alpaca".to_string(),
                        broker_stream: "market_data".to_string(),
                        updated_orders: Vec::new(),
                        message: None,
                        broker_event_type: "market_trade".to_string(),
                        broker_order_id: None,
                        market_price: price,
                        market_timestamp: Some(trade_tick.timestamp),
                        market_symbol: Some(symbol_expected.clone()),
                        payload_json,
                    };
                    if matches!(on_event(evt)?, WatchControl::Stop) {
                        return Ok(());
                    }
                }
            }
        }
    }
}
