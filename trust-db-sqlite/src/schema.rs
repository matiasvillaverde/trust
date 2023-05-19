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
    account_overviews (id) {
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

diesel::joinable!(transactions -> accounts (account_id));
diesel::joinable!(account_overviews -> accounts (account_id));
diesel::joinable!(account_overviews -> prices (total_balance_id));
diesel::joinable!(orders -> prices (price_id));
diesel::joinable!(orders -> trading_vehicles (trading_vehicle_id));
