diesel::table! {
    accounts (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        name -> Text,
        description -> Text,
        environment -> Text,
        taxes_percentage -> Text,
        earnings_percentage -> Text,
    }
}

diesel::table! {
    accounts_balances (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        account_id -> Text,
        total_balance -> Text,
        total_in_trade -> Text,
        total_available -> Text,
        taxed -> Text,
        currency -> Text,
        total_earnings -> Text,
    }
}

diesel::table! {
    rules (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        name -> Text,
        risk -> Integer,
        description -> Text,
        priority -> Integer,
        level -> Text,
        account_id -> Text,
        active -> Bool,
    }
}

diesel::table! {
    transactions (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        currency -> Text,
        category -> Text,
        amount -> Text,
        account_id -> Text,
        trade_id -> Nullable<Text>,
    }
}

diesel::table! {
    trading_vehicles (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        symbol -> Text,
        isin -> Text,
        category -> Text,
        broker -> Text,
    }
}

diesel::table! {
    orders (id) {
        id -> Text,
        broker_order_id -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        unit_price -> Text,
        currency -> Text,
        quantity -> BigInt,
        category -> Text,
        trading_vehicle_id -> Text,
        action -> Text,
        status -> Text,
        time_in_force  -> Text,
        trailing_percentage -> Nullable<Text>,
        trailing_price -> Nullable<Text>,
        filled_quantity -> BigInt,
        average_filled_price-> Nullable<Text>,
        extended_hours-> Bool,
        submitted_at -> Nullable<Timestamp>,
        filled_at -> Nullable<Timestamp>,
        expired_at -> Nullable<Timestamp>,
        cancelled_at -> Nullable<Timestamp>,
        closed_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    trades (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        category -> Text,
        status -> Text,
        currency -> Text,
        trading_vehicle_id -> Text,
        safety_stop_id -> Text,
        entry_id -> Text,
        target_id -> Text,
        account_id -> Text,
        balance_id -> Text,
        thesis -> Nullable<Text>,
        sector -> Nullable<Text>,
        asset_class -> Nullable<Text>,
        context -> Nullable<Text>,
    }
}

diesel::table! {
    trades_balances (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        currency -> Text,
        funding -> Text,
        capital_in_market -> Text,
        capital_out_market -> Text,
        taxed -> Text,
        total_performance -> Text,
    }
}

diesel::table! {
    level_changes (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        account_id -> Text,
        old_level -> Integer,
        new_level -> Integer,
        change_reason -> Text,
        trigger_type -> Text,
        changed_at -> Timestamp,
    }
}

diesel::table! {
    levels (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        account_id -> Text,
        current_level -> Integer,
        risk_multiplier -> Text,
        status -> Text,
        trades_at_level -> Integer,
        level_start_date -> Date,
    }
}

diesel::table! {
    logs (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        log -> Text,
        trade_id -> Text,
    }
}

diesel::joinable!(transactions -> accounts (account_id));
diesel::joinable!(accounts_balances -> accounts (account_id));
diesel::joinable!(level_changes -> accounts (account_id));
diesel::joinable!(levels -> accounts (account_id));
diesel::joinable!(orders -> trading_vehicles (trading_vehicle_id));
diesel::joinable!(trades -> accounts (account_id));
diesel::joinable!(trades -> trades_balances (balance_id));
diesel::joinable!(trades -> trading_vehicles (trading_vehicle_id));
diesel::joinable!(trades -> orders (safety_stop_id));
diesel::joinable!(logs -> trades (trade_id));

diesel::allow_tables_to_appear_in_same_query!(
    accounts,
    accounts_balances,
    level_changes,
    levels,
    logs,
    orders,
    rules,
    trades,
    trades_balances,
    trading_vehicles,
    transactions,
);
