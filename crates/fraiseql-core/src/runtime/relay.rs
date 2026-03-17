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

/// Encode a BIGINT primary key value as a Relay edge cursor.
///
/// The cursor is `base64(pk_string)` where `pk_string` is the decimal
/// representation of the BIGINT.  This is opaque to the client.
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
/// The cursor is `base64(uuid_string)`, opaque to the client.
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

/// Encode a global Node ID as a Relay-compatible opaque ID.
///
/// The format is `base64("TypeName:uuid")`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_cursor_roundtrip() {
        for pk in [0_i64, 1, 42, 999_999, i64::MAX] {
            let cursor = encode_edge_cursor(pk);
            assert_eq!(decode_edge_cursor(&cursor), Some(pk));
        }
    }

    #[test]
    fn test_edge_cursor_negative_pk() {
        // Negative pks are unusual but still encodable.
        let cursor = encode_edge_cursor(-1);
        assert_eq!(decode_edge_cursor(&cursor), Some(-1));
    }

    #[test]
    fn test_edge_cursor_i64_min_roundtrips() {
        // Guards the sign-flip mutation: decode(encode(i64::MIN)) must equal i64::MIN.
        let cursor = encode_edge_cursor(i64::MIN);
        assert_eq!(
            decode_edge_cursor(&cursor),
            Some(i64::MIN),
            "i64::MIN must roundtrip through encode/decode"
        );
    }

    #[test]
    fn test_edge_cursor_negative_max_roundtrips() {
        // Guards -(i64::MAX): distinct from i64::MIN, covers the full negative range.
        let cursor = encode_edge_cursor(-i64::MAX);
        assert_eq!(decode_edge_cursor(&cursor), Some(-i64::MAX));
    }

    #[test]
    fn test_edge_cursor_invalid() {
        assert_eq!(decode_edge_cursor("!!!not-base64"), None);
        assert_eq!(decode_edge_cursor(""), None);
        // Valid base64 but not an integer.
        let bad = BASE64.encode("not-a-number");
        assert_eq!(decode_edge_cursor(&bad), None);
    }

    #[test]
    fn test_node_id_roundtrip() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let id = encode_node_id("User", uuid);
        let decoded = decode_node_id(&id);
        assert_eq!(decoded, Some(("User".to_string(), uuid.to_string())));
    }

    #[test]
    fn test_node_id_various_types() {
        for type_name in ["User", "BlogPost", "OrderItem"] {
            let uuid = "00000000-0000-0000-0000-000000000001";
            let id = encode_node_id(type_name, uuid);
            let decoded = decode_node_id(&id);
            assert_eq!(decoded.as_ref().map(|(t, _)| t.as_str()), Some(type_name));
            assert_eq!(decoded.as_ref().map(|(_, u)| u.as_str()), Some(uuid));
        }
    }

    #[test]
    fn test_node_id_invalid() {
        assert_eq!(decode_node_id("!!!not-base64"), None);
        assert_eq!(decode_node_id(""), None);
        // Valid base64 but no colon separator.
        let no_colon = BASE64.encode("UserMissingColon");
        assert_eq!(decode_node_id(&no_colon), None);
    }

    #[test]
    fn test_edge_cursor_is_base64() {
        let cursor = encode_edge_cursor(42);
        // Verify it's valid base64 by decoding.
        BASE64.decode(&cursor).unwrap_or_else(|e| panic!("expected valid base64 edge cursor: {e}"));
    }

    #[test]
    fn test_node_id_is_base64() {
        let id = encode_node_id("User", "some-uuid");
        BASE64.decode(&id).unwrap_or_else(|e| panic!("expected valid base64 node ID: {e}"));
    }
}
