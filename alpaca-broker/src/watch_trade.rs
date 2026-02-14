use crate::keys;
use crate::order_mapper;
use apca::api::v2::order::{Get, Id, Order as AlpacaOrder};
use apca::api::v2::updates;
use apca::data::v2::stream::{MarketData, RealtimeData, IEX};
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

type OrderUpdatesStream = <updates::OrderUpdates as Subscribable>::Stream;
type OrderUpdatesSub = <updates::OrderUpdates as Subscribable>::Subscription;
type MarketDataStream = <RealtimeData<IEX> as Subscribable>::Stream;
type MarketDataSub = <RealtimeData<IEX> as Subscribable>::Subscription;

const BROKER_SOURCE: &str = "alpaca";
const STREAM_TRADE_UPDATES: &str = "trade_updates";
const STREAM_TRADING_REST: &str = "trading_rest";
const STREAM_MARKET_DATA: &str = "market_data";

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

#[derive(Debug, Clone)]
struct OrdersState {
    entry: Order,
    stop: Order,
    target: Order,
}

impl OrdersState {
    fn new(trade: &Trade) -> Self {
        Self {
            entry: trade.entry.clone(),
            stop: trade.safety_stop.clone(),
            target: trade.target.clone(),
        }
    }

    fn ensure_broker_ids_present(&self) -> Result<(), Box<dyn Error>> {
        if self.entry.broker_order_id.is_none()
            || self.stop.broker_order_id.is_none()
            || self.target.broker_order_id.is_none()
        {
            return Err(
                "Trade orders are missing broker_order_id; submit the trade before watching".into(),
            );
        }
        Ok(())
    }

    fn is_terminal(&self) -> bool {
        is_terminal(&self.entry, &self.stop, &self.target)
    }

    fn apply_reconciled_order(
        &mut self,
        id: Uuid,
        alpaca_order: AlpacaOrder,
        symbol_expected: &str,
    ) -> Result<Option<Order>, Box<dyn Error>> {
        if alpaca_order.symbol.to_uppercase() != symbol_expected {
            return Ok(None);
        }

        if Some(id) == self.entry.broker_order_id {
            let mapped = order_mapper::map_single(&alpaca_order, &self.entry)?;
            if mapped != self.entry {
                self.entry = mapped.clone();
                return Ok(Some(mapped));
            }
        } else if Some(id) == self.stop.broker_order_id {
            let mapped = order_mapper::map_single(&alpaca_order, &self.stop)?;
            if mapped != self.stop {
                self.stop = mapped.clone();
                return Ok(Some(mapped));
            }
        } else if Some(id) == self.target.broker_order_id {
            let mapped = order_mapper::map_single(&alpaca_order, &self.target)?;
            if mapped != self.target {
                self.target = mapped.clone();
                return Ok(Some(mapped));
            }
        }

        Ok(None)
    }

    fn apply_trade_update(
        &mut self,
        update: &updates::OrderUpdate,
        symbol_expected: &str,
    ) -> Result<(Option<Uuid>, Vec<Order>), Box<dyn Error>> {
        if update.order.symbol.to_uppercase() != symbol_expected {
            return Ok((None, Vec::new()));
        }

        let broker_order_id = Some(update.order.id.0);
        let mut updated_orders = Vec::new();

        if broker_order_id == self.entry.broker_order_id {
            let mapped = order_mapper::map_single(&update.order, &self.entry)?;
            if mapped != self.entry {
                self.entry = mapped.clone();
                updated_orders.push(mapped);
            }
        } else if broker_order_id == self.stop.broker_order_id {
            let mapped = order_mapper::map_single(&update.order, &self.stop)?;
            if mapped != self.stop {
                self.stop = mapped.clone();
                updated_orders.push(mapped);
            }
        } else if broker_order_id == self.target.broker_order_id {
            let mapped = order_mapper::map_single(&update.order, &self.target)?;
            if mapped != self.target {
                self.target = mapped.clone();
                updated_orders.push(mapped);
            }
        } else {
            return Ok((None, Vec::new()));
        }

        Ok((broker_order_id, updated_orders))
    }
}

fn reconcile_event(message: &str, updated_orders: Vec<Order>) -> WatchEvent {
    WatchEvent {
        broker_source: BROKER_SOURCE.to_string(),
        broker_stream: STREAM_TRADING_REST.to_string(),
        updated_orders,
        message: Some(message.to_string()),
        broker_event_type: "reconcile".to_string(),
        broker_order_id: None,
        market_price: None,
        market_timestamp: None,
        market_symbol: None,
        payload_json: "{}".to_string(),
    }
}

fn market_trade_event(
    symbol_expected: &str,
    trade_tick: apca::data::v2::stream::Trade,
) -> WatchEvent {
    let price = rust_decimal::Decimal::from_str(&trade_tick.trade_price.to_string()).ok();
    let payload_json = serde_json::to_string(&serde_json::json!({
        "t": trade_tick.timestamp.to_rfc3339(),
        "p": trade_tick.trade_price.to_string(),
        "s": trade_tick.trade_size.to_string(),
        "symbol": symbol_expected,
    }))
    .unwrap_or_else(|_| "{}".to_string());

    WatchEvent {
        broker_source: BROKER_SOURCE.to_string(),
        broker_stream: STREAM_MARKET_DATA.to_string(),
        updated_orders: Vec::new(),
        message: None,
        broker_event_type: "market_trade".to_string(),
        broker_order_id: None,
        market_price: price,
        market_timestamp: Some(trade_tick.timestamp),
        market_symbol: Some(symbol_expected.to_string()),
        payload_json,
    }
}

async fn reconcile_orders(
    client: &Client,
    orders: &mut OrdersState,
    symbol_expected: &str,
) -> Result<Vec<Order>, Box<dyn Error>> {
    let reconciled = reconcile_once(client, &orders.entry, &orders.stop, &orders.target).await?;
    let mut updated = Vec::new();
    for (id, alpaca_order) in reconciled {
        if let Some(order) = orders.apply_reconciled_order(id, alpaca_order, symbol_expected)? {
            updated.push(order);
        }
    }
    Ok(updated)
}

fn should_stop(control: WatchControl) -> bool {
    matches!(control, WatchControl::Stop)
}

fn can_try_connect(state: &BrokerState, now: Instant) -> bool {
    match state {
        BrokerState::ErrorRecovery { next_retry, .. } => now >= *next_retry,
        _ => true,
    }
}

fn transition_or(state: BrokerState, transition: StateTransition, now: Instant) -> BrokerState {
    state
        .transition_at(transition, now)
        .unwrap_or(BrokerState::Disconnected)
}

async fn maybe_emit_reconcile(
    client: &Client,
    orders: &mut OrdersState,
    symbol_expected: &str,
    message: &str,
    on_event: &mut dyn FnMut(WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
) -> Result<bool, Box<dyn Error>> {
    let updated_orders = match reconcile_orders(client, orders, symbol_expected).await {
        Ok(updated) => updated,
        Err(_) => return Ok(false),
    };

    if updated_orders.is_empty() {
        return Ok(false);
    }

    let evt = reconcile_event(message, updated_orders);
    Ok(should_stop(on_event(evt)?))
}

#[allow(clippy::too_many_arguments)]
async fn maybe_connect_trade_updates(
    client: &Client,
    now: Instant,
    ws_state: &mut BrokerState,
    ws_stream: &mut Option<std::pin::Pin<Box<OrderUpdatesStream>>>,
    ws_sub: &mut Option<OrderUpdatesSub>,
    orders: &mut OrdersState,
    symbol_expected: &str,
    on_event: &mut dyn FnMut(WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
) -> Result<bool, Box<dyn Error>> {
    if ws_stream.is_some() || !can_try_connect(ws_state, now) {
        return Ok(false);
    }

    *ws_state = transition_or(ws_state.clone(), StateTransition::Connect, now);

    match client.subscribe::<updates::OrderUpdates>().await {
        Ok((stream, sub)) => {
            *ws_stream = Some(Box::pin(stream));
            *ws_sub = Some(sub);
            *ws_state = transition_or(
                ws_state.clone(),
                StateTransition::ConnectionEstablished,
                now,
            );

            let stop = maybe_emit_reconcile(
                client,
                orders,
                symbol_expected,
                "ws_connected_reconcile",
                on_event,
            )
            .await?;

            *ws_state = transition_or(
                ws_state.clone(),
                StateTransition::ReconciliationComplete,
                now,
            );
            Ok(stop)
        }
        Err(_) => {
            *ws_stream = None;
            *ws_sub = None;
            *ws_state = transition_or(ws_state.clone(), StateTransition::Error, now);
            Ok(false)
        }
    }
}

async fn maybe_connect_market_data(
    client: &Client,
    now: Instant,
    md_state: &mut BrokerState,
    md_stream: &mut Option<std::pin::Pin<Box<MarketDataStream>>>,
    md_sub: &mut Option<MarketDataSub>,
    market_data_req: &MarketData,
) -> Result<(), Box<dyn Error>> {
    if md_stream.is_some() || !can_try_connect(md_state, now) {
        return Ok(());
    }

    *md_state = transition_or(md_state.clone(), StateTransition::Connect, now);

    match client.subscribe::<RealtimeData<IEX>>().await {
        Ok((stream, mut sub)) => match sub.subscribe(market_data_req).await {
            Ok(Ok(())) => {
                *md_stream = Some(Box::pin(stream));
                *md_sub = Some(sub);
                *md_state = BrokerState::Live {
                    connected_since: now,
                };
                Ok(())
            }
            _ => {
                *md_stream = None;
                *md_sub = None;
                *md_state = transition_or(md_state.clone(), StateTransition::Error, now);
                Ok(())
            }
        },
        Err(_) => {
            *md_stream = None;
            *md_sub = None;
            *md_state = transition_or(md_state.clone(), StateTransition::Error, now);
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_trade_updates_item<E>(
    item: Option<Result<Result<updates::OrderUpdate, serde_json::Error>, E>>,
    now: Instant,
    ws_state: &mut BrokerState,
    ws_stream: &mut Option<std::pin::Pin<Box<OrderUpdatesStream>>>,
    ws_sub: &mut Option<OrderUpdatesSub>,
    orders: &mut OrdersState,
    symbol_expected: &str,
    on_event: &mut dyn FnMut(WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
) -> Result<bool, Box<dyn Error>> {
    let msg = match item {
        Some(msg) => msg,
        None => {
            *ws_stream = None;
            *ws_sub = None;
            *ws_state = transition_or(ws_state.clone(), StateTransition::Error, now);
            return Ok(false);
        }
    };

    let update = match msg {
        Ok(Ok(update)) => update,
        Ok(Err(_)) => return Ok(false),
        Err(_) => {
            *ws_stream = None;
            *ws_sub = None;
            *ws_state = transition_or(ws_state.clone(), StateTransition::Error, now);
            return Ok(false);
        }
    };

    let (broker_order_id, updated_orders) = orders.apply_trade_update(&update, symbol_expected)?;
    if updated_orders.is_empty() {
        return Ok(false);
    }

    let evt = WatchEvent {
        broker_source: BROKER_SOURCE.to_string(),
        broker_stream: STREAM_TRADE_UPDATES.to_string(),
        updated_orders,
        message: None,
        broker_event_type: event_type_str(update.event).to_string(),
        broker_order_id,
        market_price: None,
        market_timestamp: None,
        market_symbol: None,
        payload_json: serde_json::to_string(&update).unwrap_or_else(|_| "{}".to_string()),
    };

    Ok(should_stop(on_event(evt)?))
}

async fn handle_market_data_item<E>(
    item: Option<Result<Result<apca::data::v2::stream::Data, serde_json::Error>, E>>,
    now: Instant,
    md_state: &mut BrokerState,
    md_stream: &mut Option<std::pin::Pin<Box<MarketDataStream>>>,
    md_sub: &mut Option<MarketDataSub>,
    symbol_expected: &str,
    on_event: &mut dyn FnMut(WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
) -> Result<bool, Box<dyn Error>> {
    let msg = match item {
        Some(msg) => msg,
        None => {
            *md_stream = None;
            *md_sub = None;
            *md_state = transition_or(md_state.clone(), StateTransition::Error, now);
            return Ok(false);
        }
    };

    let data = match msg {
        Ok(Ok(data)) => data,
        Ok(Err(_)) => return Ok(false),
        Err(_) => {
            *md_stream = None;
            *md_sub = None;
            *md_state = transition_or(md_state.clone(), StateTransition::Error, now);
            return Ok(false);
        }
    };

    if let apca::data::v2::stream::Data::Trade(trade_tick) = data {
        let evt = market_trade_event(symbol_expected, trade_tick);
        return Ok(should_stop(on_event(evt)?));
    }

    Ok(false)
}

#[allow(clippy::too_many_lines)]
async fn watch_async(
    trade: &Trade,
    client: Client,
    options: WatchOptions,
    on_event: &mut dyn FnMut(WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let mut orders = OrdersState::new(trade);
    orders.ensure_broker_ids_present()?;
    let symbol_expected = trade.trading_vehicle.symbol.to_uppercase();
    let start = Instant::now();

    // Setup periodic reconciliation (REST).
    let mut reconcile_tick = time::interval(options.reconcile_every);
    reconcile_tick.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

    // Note: we intentionally keep subscriptions alive; dropping them may close the socket.
    let mut ws_state = BrokerState::Disconnected;
    let mut ws_stream: Option<std::pin::Pin<Box<OrderUpdatesStream>>> = None;
    let mut ws_sub: Option<OrderUpdatesSub> = None;

    let mut md_state = BrokerState::Disconnected;
    let mut md_stream: Option<std::pin::Pin<Box<MarketDataStream>>> = None;
    let mut md_sub: Option<MarketDataSub> = None;

    let mut market_data_req = MarketData::default();
    market_data_req.set_trades(vec![symbol_expected.clone()]);

    if maybe_emit_reconcile(
        &client,
        &mut orders,
        &symbol_expected,
        "initial_reconcile",
        on_event,
    )
    .await?
    {
        return Ok(());
    }

    loop {
        if let Some(timeout) = options.timeout {
            if start.elapsed() > timeout {
                return Ok(());
            }
        }

        if orders.is_terminal() {
            return Ok(());
        }

        let now = Instant::now();

        if maybe_connect_trade_updates(
            &client,
            now,
            &mut ws_state,
            &mut ws_stream,
            &mut ws_sub,
            &mut orders,
            &symbol_expected,
            on_event,
        )
        .await?
        {
            return Ok(());
        }

        maybe_connect_market_data(
            &client,
            now,
            &mut md_state,
            &mut md_stream,
            &mut md_sub,
            &market_data_req,
        )
        .await?;

        let ws_backoff = ws_state.backoff_duration();
        let md_backoff = md_state.backoff_duration();
        let idle_sleep = ws_backoff
            .min(md_backoff)
            .max(std::time::Duration::from_millis(100));

        tokio::select! {
            _ = reconcile_tick.tick() => {
                if maybe_emit_reconcile(&client, &mut orders, &symbol_expected, "reconcile", on_event).await? {
                    return Ok(());
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
                if handle_trade_updates_item(
                    maybe,
                    now,
                    &mut ws_state,
                    &mut ws_stream,
                    &mut ws_sub,
                    &mut orders,
                    &symbol_expected,
                    on_event,
                )
                .await?
                {
                    return Ok(());
                }
            }

            maybe = async {
                match md_stream.as_mut() {
                    Some(stream) => stream.as_mut().next().await,
                    None => std::future::pending().await,
                }
            } => {
                if handle_market_data_item(
                    maybe,
                    now,
                    &mut md_state,
                    &mut md_stream,
                    &mut md_sub,
                    &symbol_expected,
                    on_event,
                )
                .await?
                {
                    return Ok(());
                }
            }
        }
    }
}
