//! Exact-text decoding of PostgreSQL's binary `NUMERIC` wire format.
//!
//! `NUMERIC`/`DECIMAL` result columns arrive as PostgreSQL's on-the-wire
//! numeric struct — sign word, base-10000 digit groups, a decimal-point weight
//! and a display scale — and [`PgNumericText`] renders that to the same text
//! PostgreSQL itself prints for `value::text` (`get_str_from_var` in the
//! server's `numeric.c`). Decoding to text keeps the full precision of the
//! stored value: the previous decoder (`rust_decimal::Decimal`, removed in
//! #980) capped at 28-29 significant digits and had no `NaN`/`Infinity`, so
//! anything wider fell through [`super::database`]'s type ladder to `Null`.
//!
//! The rendering is verified against PostgreSQL directly: the
//! `numeric_decode_matches_postgres_own_text_rendering` integration test
//! compares this decoder's output with `SELECT value::text` from the same
//! server over a curated-plus-generated corpus.

use tokio_postgres::types::{FromSql, Type};

#[cfg(test)]
mod tests;

/// Sign-word values of the binary NUMERIC header (`numeric.c`).
const NUMERIC_POS: u16 = 0x0000;
const NUMERIC_NEG: u16 = 0x4000;
const NUMERIC_NAN: u16 = 0xC000;
const NUMERIC_PINF: u16 = 0xD000;
const NUMERIC_NINF: u16 = 0xF000;

/// A `NUMERIC` value decoded to PostgreSQL's exact text rendering.
pub(super) struct PgNumericText(pub(super) String);

impl<'a> FromSql<'a> for PgNumericText {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        decode_numeric_text(raw).map(Self)
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::NUMERIC
    }
}

/// Render a binary NUMERIC wire value as PostgreSQL renders it in text.
///
/// Layout: `u16 ndigits`, `i16 weight` (index of the first digit group relative
/// to the decimal point, in base-10000 groups), `u16 sign`, `u16 dscale`, then
/// `ndigits` big-endian `u16` groups each in `0..=9999`. A malformed buffer is
/// an error, never a guess — a real server cannot produce one, so any occurrence
/// means corruption and must surface loudly.
fn decode_numeric_text(
    raw: &[u8],
) -> std::result::Result<String, Box<dyn std::error::Error + Sync + Send>> {
    use std::fmt::Write as _;

    if raw.len() < 8 {
        return Err(format!(
            "NUMERIC wire value is {} bytes, shorter than its 8-byte header",
            raw.len()
        )
        .into());
    }
    let ndigits = usize::from(u16::from_be_bytes([raw[0], raw[1]]));
    let weight = i32::from(i16::from_be_bytes([raw[2], raw[3]]));
    let sign = u16::from_be_bytes([raw[4], raw[5]]);
    let dscale = usize::from(u16::from_be_bytes([raw[6], raw[7]]) & 0x3FFF);

    match sign {
        NUMERIC_NAN => return Ok("NaN".to_string()),
        NUMERIC_PINF => return Ok("Infinity".to_string()),
        NUMERIC_NINF => return Ok("-Infinity".to_string()),
        NUMERIC_POS | NUMERIC_NEG => {},
        other => {
            return Err(
                format!("NUMERIC wire value has unrecognised sign word {other:#06x}").into()
            );
        },
    }

    if raw.len() != 8 + 2 * ndigits {
        return Err(format!(
            "NUMERIC wire value declares {ndigits} digit groups but carries {} bytes after the header",
            raw.len() - 8
        )
        .into());
    }
    let mut digits = Vec::with_capacity(ndigits);
    for i in 0..ndigits {
        let group = u16::from_be_bytes([raw[8 + 2 * i], raw[9 + 2 * i]]);
        if group > 9999 {
            return Err(format!("NUMERIC digit group {group} exceeds base-10000").into());
        }
        digits.push(group);
    }
    // A base-10000 group at position `d` (0 = the group just left of the
    // decimal point when weight = 0); positions outside the stored groups are
    // implicit zeros on both ends.
    let group_at = |d: i32| -> u16 {
        usize::try_from(d).ok().and_then(|i| digits.get(i)).copied().unwrap_or(0)
    };

    let mut out = String::new();
    if sign == NUMERIC_NEG {
        out.push('-');
    }

    // Integer part: the groups from position 0 through `weight`, the first
    // without leading zeros. A value entirely below 1 has weight < 0 and prints
    // a bare "0".
    if weight < 0 {
        out.push('0');
    } else {
        for d in 0..=weight {
            let group = group_at(d);
            // Reason (expect): fmt::Write for String is infallible.
            if d == 0 {
                write!(out, "{group}").expect("write to String");
            } else {
                write!(out, "{group:04}").expect("write to String");
            }
        }
    }

    // Fraction part: exactly `dscale` digits, padding with zeros past the last
    // stored group and truncating inside the final group — identical to how the
    // server pads/rounds the display scale.
    if dscale > 0 {
        out.push('.');
        let mut written = 0;
        let mut d = weight + 1;
        while written < dscale {
            let group = group_at(d);
            let rendered = format!("{group:04}");
            let take = (dscale - written).min(4);
            out.push_str(&rendered[..take]);
            written += take;
            d += 1;
        }
    }

    Ok(out)
}
