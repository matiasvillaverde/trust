use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use uuid::Uuid;

/// Event emitted when a trade is closed.
#[derive(Debug, Clone, PartialEq)]
pub struct TradeClosed {
    /// Closed trade identifier.
    pub trade_id: Uuid,
    /// Account that owns the trade.
    pub account_id: Uuid,
    /// Final realized PnL of the trade.
    pub final_pnl: Decimal,
    /// Trade R-multiple at close.
    pub r_multiple: Decimal,
    /// Why the trade was closed.
    pub close_reason: CloseReason,
    /// Close timestamp.
    pub closed_at: NaiveDateTime,
}

/// Reason why a trade was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// Trade closed at target.
    Target,
    /// Trade closed at stop.
    StopLoss,
    /// Trade closed at stop with slippage.
    StopLossSlippage,
    /// Trade closed manually.
    Manual,
}
