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
    prices(id) {
        id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        currency -> Text,
        amount -> Text,
    }
}
