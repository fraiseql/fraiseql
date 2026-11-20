//! `timestamptz` for tokio-postgres, through postgres-types.

use std::error::Error as StdError;

use bytes::{BufMut, BytesMut};
use postgres_types::{FromSql, IsNull, ToSql, Type, accepts, to_sql_checked};

use crate::{
    Timestamp,
    pg::{from_pg_microseconds, to_pg_microseconds},
};

type BoxError = Box<dyn StdError + Sync + Send>;

impl<'a> FromSql<'a> for Timestamp {
    accepts!(TIMESTAMPTZ);

    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, BoxError> {
        let bytes: [u8; 8] = raw
            .try_into()
            .map_err(|_| format!("a timestamptz is 8 bytes, this value is {}", raw.len()))?;
        Ok(from_pg_microseconds(i64::from_be_bytes(bytes))?)
    }
}

impl ToSql for Timestamp {
    accepts!(TIMESTAMPTZ);

    to_sql_checked!();

    fn to_sql(&self, _: &Type, out: &mut BytesMut) -> Result<IsNull, BoxError> {
        out.put_i64(to_pg_microseconds(*self));
        Ok(IsNull::No)
    }
}
