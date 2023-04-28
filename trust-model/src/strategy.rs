use chrono::NaiveDateTime;
use uuid::Uuid;

/// Strategy entity - represents a single strategy
/// A strategy is a set of rules that are used to identify trading opportunities.
/// A strategy can be used to identify entries, exits, targets, etc.
/// It is recommended to not update a strategy once it is created.
/// If you want to update a strategy, create a new one with a new version.
///
/// This will allow you to keep track of the changes.
/// For example, if you want to update the description of the strategy, create a new strategy with the same name and version + 1.
#[derive(PartialEq, Debug)]
pub struct Strategy {
    pub id: Uuid,

    // Entity timestamps
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// The name of the strategy. For example: Bullish divergence on RSI
    pub name: String,

    /// The description of the strategy
    pub description: String,

    /// The version of the strategy. For example: 1. The version is used to identify the strategy.
    pub version: u16,

    /// The entry condition of the strategy. For example: Buy in pullback.
    pub entry: String,

    /// The exit condition of the strategy. For example: Set a stop loss at 10% below the entry price.
    pub stop: String,

    /// The target condition of the strategy. For example: How to set target A, B, C.
    pub target: String,
}
