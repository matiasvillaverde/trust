//! A checkpoint cursor must preserve SQLite's original comparison domain and
//! bytes. Coercing BLOBs or replacing invalid UTF-8 could repeat or skip receipts.
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql},
    sql_types::Text,
    sqlite::{Sqlite, SqliteType},
};

pub(super) struct CursorText(String);

impl FromSql<Text, Sqlite> for CursorText {
    fn from_sql(mut value: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        if !matches!(value.value_type(), Some(SqliteType::Text)) {
            return Err("Cash ledger ordering key must be stored as text".into());
        }
        let text = std::str::from_utf8(value.read_blob())
            .map_err(|_| "Cash ledger ordering key is not valid UTF-8")?;
        Ok(Self(text.to_owned()))
    }
}

impl From<CursorText> for String {
    fn from(value: CursorText) -> Self {
        value.0
    }
}
