//! Relay cursor encoding and decoding.
//!
//! FraiseQL uses two kinds of cursors:
//!
//! ## Edge Cursor (keyset pagination)
//!
//! Used in `XxxConnection.edges[].cursor` for forward/backward pagination.
//! Encodes the BIGINT primary key (`pk_{type}`) as `base64(pk_value_decimal_string)`.
//!
//! Example: `pk_user = 42` → cursor = `base64("42")` = `"NDI="`
//!
//! A connection read with an `orderBy` resumes past the row's sort-key values, not its key
//! alone (#1521), so its cursor carries them: `base64` of a JSON object holding the row's
//! position, its sort-key values, and a fingerprint of the ordering they belong to
//! ([`KeysetCursor`]). An unordered connection keeps the plain cursor.
//!
//! ## Node ID (global object identification)
//!
//! Used in the `Node.id` field and the `node(id: ID!)` global query.
//! Encodes type name + UUID as `base64("TypeName:uuid")`.
//!
//! Example: User with UUID `"550e8400-..."` → `base64("User:550e8400-...")`.
//!
//! ## Relay spec references
//!
//! - [Global Object Identification](https://relay.dev/graphql/objectidentification.htm)
//! - [Cursor Connections](https://relay.dev/graphql/connections.htm)

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The [`KeysetCursor`] format this release issues and reads. A plain cursor (the cursor
/// column alone) is the format before it, and has no version.
pub const KEYSET_CURSOR_VERSION: u8 = 2;

/// A relay edge cursor under an `orderBy` (#1521): where its row sits in that ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeysetCursor {
    /// The format, [`KEYSET_CURSOR_VERSION`] when issued by this release.
    #[serde(rename = "v")]
    pub version:   u8,
    /// The [`ordering_fingerprint`] of the ordering the cursor was issued under.
    #[serde(rename = "o")]
    pub ordering:  String,
    /// The row's sort-key values as text, in ordering order; `None` for NULL.
    #[serde(rename = "k")]
    pub sort_keys: Vec<Option<String>>,
    /// The row's position: the connection's cursor column (an integer or a UUID string).
    #[serde(rename = "c")]
    pub position:  serde_json::Value,
}

/// A short, stable fingerprint of an ordering's signature (`fraiseql_db::keyset`): the first
/// 64 bits of its SHA-256, in hex.
#[must_use]
pub fn ordering_fingerprint(signature: &str) -> String {
    let digest = Sha256::digest(signature.as_bytes());
    let mut first = [0_u8; 8];
    first.copy_from_slice(&digest[..8]);
    format!("{:016x}", u64::from_be_bytes(first))
}

/// Encode a [`KeysetCursor`] as an opaque edge cursor.
#[must_use]
pub fn encode_keyset_cursor(cursor: &KeysetCursor) -> String {
    // Reason: a struct of strings and a JSON value always serializes.
    BASE64.encode(serde_json::to_vec(cursor).unwrap_or_default())
}

/// Decode an edge cursor issued under an ordering, of any version; `None` for any other string,
/// a plain cursor included.
#[must_use]
pub fn decode_keyset_cursor(cursor: &str) -> Option<KeysetCursor> {
    serde_json::from_slice(&BASE64.decode(cursor).ok()?).ok()
}

/// Encode a BIGINT primary key value as a Relay edge cursor.
///
/// The cursor is `base64(pk_string)` where `pk_string` is the decimal
/// representation of the BIGINT.  Base64 is encoding, not encryption —
/// a client that decodes the cursor will see the raw integer PK value.
/// The Relay spec requires cursors to be treated as opaque by convention,
/// but provides no cryptographic guarantee.
///
/// # Example
///
/// ```
/// use fraiseql_core::runtime::relay::encode_edge_cursor;
///
/// let cursor = encode_edge_cursor(42);
/// assert_eq!(cursor, base64_of("42"));
/// # fn base64_of(s: &str) -> String {
/// #     use base64::{Engine as _, engine::general_purpose::STANDARD};
/// #     STANDARD.encode(s)
/// # }
/// ```
#[must_use]
pub fn encode_edge_cursor(pk: i64) -> String {
    BASE64.encode(pk.to_string())
}

/// Decode a Relay edge cursor back to a BIGINT primary key value.
///
/// Returns `None` if the cursor is not valid base64 or does not contain a
/// valid decimal integer.
///
/// # Example
///
/// ```
/// use fraiseql_core::runtime::relay::{decode_edge_cursor, encode_edge_cursor};
///
/// let cursor = encode_edge_cursor(42);
/// assert_eq!(decode_edge_cursor(&cursor), Some(42));
/// assert_eq!(decode_edge_cursor("not-valid-base64!!"), None);
/// ```
#[must_use]
pub fn decode_edge_cursor(cursor: &str) -> Option<i64> {
    let bytes = BASE64.decode(cursor).ok()?;
    let s = std::str::from_utf8(&bytes).ok()?;
    s.parse::<i64>().ok()
}

/// Encode a UUID string as a Relay edge cursor.
///
/// The cursor is `base64(uuid_string)`.  Base64 is encoding, not encryption —
/// a client that decodes the cursor will see the raw UUID.  The Relay spec
/// requires cursors to be treated as opaque by convention, but provides no
/// cryptographic guarantee.
///
/// # Example
///
/// ```
/// use fraiseql_core::runtime::relay::{decode_uuid_cursor, encode_uuid_cursor};
///
/// let uuid = "550e8400-e29b-41d4-a716-446655440000";
/// let cursor = encode_uuid_cursor(uuid);
/// assert_eq!(decode_uuid_cursor(&cursor), Some(uuid.to_string()));
/// ```
#[must_use]
pub fn encode_uuid_cursor(uuid: &str) -> String {
    BASE64.encode(uuid)
}

/// Decode a Relay edge cursor back to a UUID string.
///
/// Returns `None` if the cursor is not valid base64 or not valid UTF-8.
///
/// # Example
///
/// ```
/// use fraiseql_core::runtime::relay::{decode_uuid_cursor, encode_uuid_cursor};
///
/// let uuid = "550e8400-e29b-41d4-a716-446655440000";
/// let cursor = encode_uuid_cursor(uuid);
/// assert_eq!(decode_uuid_cursor(&cursor), Some(uuid.to_string()));
/// assert_eq!(decode_uuid_cursor("not-valid-base64!!"), None);
/// ```
#[must_use]
pub fn decode_uuid_cursor(cursor: &str) -> Option<String> {
    let bytes = BASE64.decode(cursor).ok()?;
    std::str::from_utf8(&bytes).ok().map(str::to_owned)
}

/// Encode a global Node ID as a Relay-compatible ID.
///
/// The format is `base64("TypeName:uuid")`.  Base64 is encoding, not
/// encryption — a client that decodes the ID will see the type name and UUID.
///
/// # Example
///
/// ```
/// use fraiseql_core::runtime::relay::encode_node_id;
///
/// let id = encode_node_id("User", "550e8400-e29b-41d4-a716-446655440000");
/// // id = base64("User:550e8400-e29b-41d4-a716-446655440000")
/// assert!(!id.is_empty());
/// ```
#[must_use]
pub fn encode_node_id(type_name: &str, uuid: &str) -> String {
    BASE64.encode(format!("{type_name}:{uuid}"))
}

/// Decode a Relay global Node ID back to `(type_name, uuid)`.
///
/// Returns `None` if the ID is not valid base64 or does not have the
/// expected `"TypeName:uuid"` format.
///
/// # Example
///
/// ```
/// use fraiseql_core::runtime::relay::{decode_node_id, encode_node_id};
///
/// let id = encode_node_id("User", "550e8400-e29b-41d4-a716-446655440000");
/// let decoded = decode_node_id(&id);
/// assert_eq!(
///     decoded,
///     Some(("User".to_string(), "550e8400-e29b-41d4-a716-446655440000".to_string()))
/// );
/// ```
#[must_use]
pub fn decode_node_id(id: &str) -> Option<(String, String)> {
    let bytes = BASE64.decode(id).ok()?;
    let s = std::str::from_utf8(&bytes).ok()?;
    let (type_name, uuid) = s.split_once(':')?;
    if type_name.is_empty() || uuid.is_empty() {
        return None;
    }
    Some((type_name.to_string(), uuid.to_string()))
}
