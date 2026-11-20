//! `timestamptz` for sqlx's PostgreSQL driver.

use sqlx::{
    Decode, Encode, Postgres, Type,
    encode::IsNull,
    error::BoxDynError,
    postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueFormat, PgValueRef},
};

use crate::{
    Error, Timestamp,
    parse::parse_pg_text,
    pg::{from_pg_microseconds, to_pg_microseconds},
};

impl Type<Postgres> for Timestamp {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("timestamptz")
    }
}

impl PgHasArrayType for Timestamp {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_timestamptz")
    }
}

impl Encode<'_, Postgres> for Timestamp {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        Encode::<Postgres>::encode(to_pg_microseconds(*self), buf)
    }

    fn size_hint(&self) -> usize {
        size_of::<i64>()
    }
}

impl<'r> Decode<'r, Postgres> for Timestamp {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        match value.format() {
            PgValueFormat::Binary => {
                let microseconds = <i64 as Decode<Postgres>>::decode(value)?;
                Ok(from_pg_microseconds(microseconds)?)
            },
            PgValueFormat::Text => {
                let text = value.as_str()?;
                Ok(parse_pg_text(text).ok_or_else(|| Error::Parse {
                    input: text.to_owned(),
                })?)
            },
        }
    }
}
