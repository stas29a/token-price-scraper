// @generated automatically by Diesel CLI.

diesel::table! {
    prices (id) {
        id -> Int8,
        #[max_length = 200]
        symbol -> Varchar,
        price -> Numeric,
        created_at -> Timestamp,
    }
}
