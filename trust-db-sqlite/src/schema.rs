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
