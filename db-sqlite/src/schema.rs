// @generated automatically by Diesel CLI.

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
    executions (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        broker -> Text,
        source -> Text,
        account_id -> Text,
        trade_id -> Nullable<Text>,
        order_id -> Nullable<Text>,
        broker_execution_id -> Text,
        broker_order_id -> Nullable<Text>,
        symbol -> Text,
        side -> Text,
        qty -> Text,
        price -> Text,
        executed_at -> Timestamp,
        raw_json -> Nullable<Text>,
    }
}

diesel::table! {
    level_adjustment_rules (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        account_id -> Text,
        monthly_loss_downgrade_pct -> Text,
        single_loss_downgrade_pct -> Text,
        upgrade_profitable_trades -> Integer,
        upgrade_win_rate_pct -> Text,
        upgrade_consecutive_wins -> Integer,
        cooldown_profitable_trades -> Integer,
        cooldown_win_rate_pct -> Text,
        cooldown_consecutive_wins -> Integer,
        recovery_profitable_trades -> Integer,
        recovery_win_rate_pct -> Text,
        recovery_consecutive_wins -> Integer,
        min_trades_at_level_for_upgrade -> Integer,
        max_changes_in_30_days -> Integer,
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

diesel::table! {
    orders (id) {
        id -> Text,
        broker_order_id -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        unit_price -> Text,
        currency -> Text,
        quantity -> Integer,
        category -> Text,
        trading_vehicle_id -> Text,
        action -> Text,
        status -> Text,
        time_in_force -> Text,
        trailing_percentage -> Nullable<Text>,
        trailing_price -> Nullable<Text>,
        filled_quantity -> Nullable<Integer>,
        average_filled_price -> Nullable<Text>,
        extended_hours -> Bool,
        submitted_at -> Nullable<Timestamp>,
        filled_at -> Nullable<Timestamp>,
        expired_at -> Nullable<Timestamp>,
        cancelled_at -> Nullable<Timestamp>,
        closed_at -> Nullable<Timestamp>,
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
    trade_grades (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        trade_id -> Text,
        overall_score -> Integer,
        overall_grade -> Text,
        process_score -> Integer,
        risk_score -> Integer,
        execution_score -> Integer,
        documentation_score -> Integer,
        recommendations -> Nullable<Text>,
        graded_at -> Timestamp,
        process_weight_permille -> Integer,
        risk_weight_permille -> Integer,
        execution_weight_permille -> Integer,
        documentation_weight_permille -> Integer,
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
    trading_vehicles (id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        symbol -> Text,
        isin -> Nullable<Text>,
        category -> Text,
        broker -> Text,
        broker_asset_id -> Nullable<Text>,
        exchange -> Nullable<Text>,
        broker_asset_class -> Nullable<Text>,
        broker_asset_status -> Nullable<Text>,
        tradable -> Nullable<Bool>,
        marginable -> Nullable<Bool>,
        shortable -> Nullable<Bool>,
        easy_to_borrow -> Nullable<Bool>,
        fractionable -> Nullable<Bool>,
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

diesel::joinable!(accounts_balances -> accounts (account_id));
diesel::joinable!(executions -> accounts (account_id));
diesel::joinable!(executions -> orders (order_id));
diesel::joinable!(executions -> trades (trade_id));
diesel::joinable!(level_adjustment_rules -> accounts (account_id));
diesel::joinable!(level_changes -> accounts (account_id));
diesel::joinable!(levels -> accounts (account_id));
diesel::joinable!(logs -> trades (trade_id));
diesel::joinable!(orders -> trading_vehicles (trading_vehicle_id));
diesel::joinable!(rules -> accounts (account_id));
diesel::joinable!(trade_grades -> trades (trade_id));
diesel::joinable!(trades -> accounts (account_id));
diesel::joinable!(trades -> trades_balances (balance_id));
diesel::joinable!(trades -> trading_vehicles (trading_vehicle_id));
diesel::joinable!(transactions -> accounts (account_id));
diesel::joinable!(transactions -> trades (trade_id));

diesel::allow_tables_to_appear_in_same_query!(
    accounts,
    accounts_balances,
    executions,
    level_adjustment_rules,
    level_changes,
    levels,
    logs,
    orders,
    rules,
    trade_grades,
    trades,
    trades_balances,
    trading_vehicles,
    transactions,
);
