//! The response-bytes ceiling: `[validation] max_response_bytes`.
//!
//! # Why a byte ceiling rather than a bigger complexity score
//!
//! GATE-1 scores a *document* — depth, complexity, aliases — and multiplies a
//! nested selection by its page size at every level
//! (`graphql::complexity::DocumentAnalyzer::field_complexity`). That arithmetic
//! models an engine that resolves each nesting level with its own unit of work.
//!
//! This engine does not have one. A nested field on a materialised view is
//! already inside the parent document, and
//! [`project_nested_lists`](crate::runtime::project_nested_lists) projects it out
//! of rows that have *already been read* — one read, whatever the depth. Scoring
//! such a read by multiply-per-level over-estimates it by orders of magnitude and
//! would be bounding an execution model this framework does not use.
//!
//! What a materialised read actually costs is rows × document bytes. That is the
//! quantity this module bounds, and it is the only one that means the same thing
//! on every transport: a GraphQL document, a REST `?select=`, a gRPC column read
//! and an NDJSON export all end in rows.
//!
//! Depth and complexity keep their job — they guard fields that *are* resolved at
//! runtime, and malformed documents. They are not the control that bounds work on
//! a read that is one row fetch.
//!
//! # Why it is charged after the read and not scored before it
//!
//! Size is not recoverable from the request. The same query against the same view
//! returns a kilobyte for one tenant and a gigabyte for another, and no pre-read
//! estimate can tell them apart without reading. `max_page_size` (#421) bounds the
//! row count and `max_operation_cost` (#379) bounds what the request asks for;
//! neither knows what a row weighs.
//!
//! So the ceiling is **resolved** at the read chokepoint — it travels on the
//! resolved read like every other decision that belongs to the read rather than to
//! its delivery — and **charged** wherever the rows arrive. That split is the same
//! one `6a709bc1f` used for the per-row field gate: `RowReadPlan` carries the gate,
//! and the streaming arm applies it per frame so the transport holds no policy
//! decision of its own.
//!
//! A buffered read is charged once, on the rows the adapter returned. A streamed
//! read is charged per frame and cut at the frame that crosses the ceiling — which
//! is the point of doing it on the stream at all: it is the only arm where the
//! server would otherwise keep producing after the answer is already too big.

use fraiseql_db::types::{ColumnValue, JsonbValue};

use crate::error::{FraiseQLError, Result};

/// Structural overhead charged for one JSON object or array: the two brackets.
const BRACKETS: u64 = 2;

/// Structural overhead charged per object member: two quotes around the key, the
/// colon, and the comma that separates it from the next one.
const MEMBER_PUNCTUATION: u64 = 4;

/// Structural overhead charged per array element: the separating comma.
const ELEMENT_PUNCTUATION: u64 = 1;

/// Charged for a JSON string on top of its contents: the two quote characters.
///
/// Escaping is not modelled. A string of control characters or quotes serialises
/// longer than this counts, so the estimate can run under the true wire size on
/// pathological content. See the accuracy note on [`json_bytes`].
const QUOTES: u64 = 2;

/// Bytes charged for a JSON number.
///
/// `serde_json` keeps a number as `u64`/`i64`/`f64`, so the exact serialised width
/// is only knowable by formatting it. Formatting every number of every row to
/// measure the row is a second full serialisation of the response, which is the
/// cost this ceiling exists to avoid paying twice; 20 is the width of the widest
/// `u64` and therefore an upper bound for the integer cases.
const NUMBER_BYTES: u64 = 20;

/// Bytes charged for `null`, and for `true`/`false` (4 and 5 — the wider is used).
const LITERAL_BYTES: u64 = 5;

/// Estimated serialised size of a JSON value, in bytes.
///
/// # Accuracy
///
/// This is an estimate, deliberately. An exact count means serialising the value
/// to measure it and then serialising it again to send it, on every row — the
/// response would be built twice to enforce a ceiling on its size.
///
/// The estimate is within a few percent of the wire size for the documents this
/// engine actually returns, and the ways it can be wrong are bounded and known:
/// numbers are charged at the widest `u64` width (over-counts small integers), and
/// string escaping is not modelled (under-counts a string made of quotes or
/// control characters). Neither moves the total by an order of magnitude, which is
/// the resolution a `DoS` ceiling needs.
///
/// Callers that need the exact figure — an HTTP `Content-Length`, say — must
/// measure the serialised bytes; this is not that number and is not documented as
/// one.
///
/// # Recursion
///
/// Depth is bounded before this runs: `serde_json` refuses to parse beyond its own
/// nesting limit, so a value reaching here has already been accepted by the
/// parser. There is no separate guard.
#[must_use]
pub fn json_bytes(value: &serde_json::Value) -> u64 {
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) => LITERAL_BYTES,
        serde_json::Value::Number(_) => NUMBER_BYTES,
        serde_json::Value::String(s) => QUOTES.saturating_add(s.len() as u64),
        serde_json::Value::Array(items) => items.iter().fold(BRACKETS, |acc, item| {
            acc.saturating_add(ELEMENT_PUNCTUATION).saturating_add(json_bytes(item))
        }),
        serde_json::Value::Object(members) => members.iter().fold(BRACKETS, |acc, (k, v)| {
            acc.saturating_add(MEMBER_PUNCTUATION)
                .saturating_add(k.len() as u64)
                .saturating_add(json_bytes(v))
        }),
    }
}

/// Estimated serialised size of one row-shaped column value, in bytes.
///
/// The row transports (gRPC) encode a column as protobuf rather than JSON, so the
/// punctuation constants above do not apply. What is charged is the payload — the
/// only part that scales with the data — plus a fixed width for the scalars.
#[must_use]
pub const fn column_bytes(value: &ColumnValue) -> u64 {
    match value {
        ColumnValue::Text(s)
        | ColumnValue::Uuid(s)
        | ColumnValue::Timestamptz(s)
        | ColumnValue::Date(s)
        | ColumnValue::Json(s) => s.len() as u64,
        ColumnValue::Int32(_) | ColumnValue::Float64(_) | ColumnValue::Int64(_) => 8,
        ColumnValue::Boolean(_) => 1,
        ColumnValue::Null => 0,
    }
}

/// A running total of delivered bytes, against the ceiling the read resolved.
///
/// Held by value and charged as rows arrive. A streamed read keeps one across its
/// whole lifetime — that is what makes the ceiling bound the *response* rather than
/// each frame independently.
#[derive(Debug, Clone)]
pub struct ResponseBudget {
    limit: u64,
    used:  u64,
}

impl ResponseBudget {
    /// A budget for a resolved ceiling, or `None` when the operator declared none.
    ///
    /// Returning `Option` rather than an unbounded budget keeps "no ceiling
    /// configured" a shape the caller has to handle, instead of a `u64::MAX` that
    /// silently behaves like one until it overflows.
    #[must_use]
    pub const fn new(limit: Option<u64>) -> Option<Self> {
        match limit {
            Some(limit) => Some(Self { limit, used: 0 }),
            None => None,
        }
    }

    /// Bytes charged so far.
    #[must_use]
    pub const fn used(&self) -> u64 {
        self.used
    }

    /// The ceiling this budget enforces.
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Add `bytes` to the running total, refusing once it crosses the ceiling.
    ///
    /// The total is updated before the comparison, so the `bytes` reported in the
    /// error is the figure that crossed the line rather than the last value under
    /// it.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::ResponseTooLarge`] once the accumulated total exceeds the
    /// ceiling.
    pub const fn charge(&mut self, bytes: u64) -> Result<()> {
        self.used = self.used.saturating_add(bytes);
        if self.used > self.limit {
            return Err(FraiseQLError::ResponseTooLarge {
                bytes: self.used,
                limit: self.limit,
            });
        }
        Ok(())
    }

    /// Charge a buffered JSON read.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::ResponseTooLarge`] when the rows exceed the ceiling.
    pub fn charge_jsonb_rows(&mut self, rows: &[JsonbValue]) -> Result<()> {
        for row in rows {
            self.charge(json_bytes(row.as_value()))?;
        }
        Ok(())
    }

    /// Charge one row-shaped frame.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::ResponseTooLarge`] when the frame takes the response over
    /// the ceiling.
    pub fn charge_column_row(&mut self, row: &[ColumnValue]) -> Result<()> {
        let bytes = row.iter().map(column_bytes).fold(0u64, u64::saturating_add);
        self.charge(bytes)
    }
}

#[cfg(test)]
mod tests;
