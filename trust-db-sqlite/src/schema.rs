diesel::table! {
    accounts (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        name -> Text,
        description -> Text,
        environment -> Text,
    }
}

diesel::table! {
    accounts_overviews (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        account_id -> Text,
        total_balance_id -> Text,
        total_in_trade_id -> Text,
        total_available_id -> Text,
        taxed_id -> Text,
        currency -> Text,
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
    prices(id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        currency -> Text,
        amount -> Text,
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
        price_id -> Text,
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
        price_id -> Text,
        quantity -> BigInt,
        category -> Text,
        trading_vehicle_id -> Text,
        action -> Text,
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
        overview_id -> Text,
    }
}

diesel::table! {
    trades_overviews (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        funding_id -> Text,
        capital_in_market_id -> Text,
        capital_out_market_id -> Text,
        taxed_id -> Text,
        total_performance_id -> Text,
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
diesel::joinable!(accounts_overviews -> accounts (account_id));
diesel::joinable!(accounts_overviews -> prices (total_balance_id));
diesel::joinable!(orders -> prices (price_id));
diesel::joinable!(orders -> trading_vehicles (trading_vehicle_id));
diesel::joinable!(trades -> accounts (account_id));
diesel::joinable!(trades -> trades_overviews (overview_id));
diesel::joinable!(trades -> trading_vehicles (trading_vehicle_id));
diesel::joinable!(trades -> orders (safety_stop_id));
diesel::joinable!(logs -> trades (trade_id));
