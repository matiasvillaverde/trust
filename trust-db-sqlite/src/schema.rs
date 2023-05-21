diesel::table! {
    accounts (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        name -> Text,
        description -> Text,
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
        total_taxable_id -> Text,
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
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        price_id -> Text,
        quantity -> BigInt,
        trading_vehicle_id -> Text,
        action -> Text,
        category -> Text,
        opened_at -> Nullable<Timestamp>,
        closed_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    targets (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        target_price_id -> Text,
        order_id -> Text,
        trade_id -> Text,
    }
}

diesel::table! {
    trades (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        category -> Text,
        currency -> Text,
        trading_vehicle_id -> Text,
        safety_stop_id -> Text,
        entry_id -> Text,
        account_id -> Text,
        lifecycle_id -> Text,
        overview_id -> Text,
    }
}

diesel::table! {
    trades_lifecycle (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        approved_at -> Nullable<Timestamp>,
        rejected_at -> Nullable<Timestamp>,
        executed_at -> Nullable<Timestamp>,
        failed_at -> Nullable<Timestamp>,
        closed_at -> Nullable<Timestamp>,
        rejected_by_rule_id -> Nullable<Text>,
    }
}

diesel::table! {
    trades_overviews (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        total_input_id -> Text,
        total_in_market_id -> Text,
        total_out_market_id -> Text,
        total_taxable_id -> Text,
        total_performance_id -> Text,
    }
}

diesel::joinable!(transactions -> accounts (account_id));
diesel::joinable!(accounts_overviews -> accounts (account_id));
diesel::joinable!(accounts_overviews -> prices (total_balance_id));
diesel::joinable!(orders -> prices (price_id));
diesel::joinable!(orders -> trading_vehicles (trading_vehicle_id));
diesel::joinable!(targets -> prices (target_price_id));
diesel::joinable!(targets -> orders (order_id));
diesel::joinable!(targets -> trades (trade_id));
diesel::joinable!(trades -> accounts (account_id));
diesel::joinable!(trades -> trades_lifecycle (lifecycle_id));
diesel::joinable!(trades -> trades_overviews (overview_id));
diesel::joinable!(trades -> trading_vehicles (trading_vehicle_id));
diesel::joinable!(trades -> orders (safety_stop_id));
diesel::joinable!(trades_lifecycle -> rules (rejected_by_rule_id));
