#![no_main]
//! The four Relay decoders, which read cursors and node ids straight from a client's
//! query arguments: keyset, edge (BIGINT), UUID cursors, and global node ids.
//!
//! Beyond "no panic", each must round-trip. A value a decoder accepts, encoded again,
//! decodes to the same value, so a cursor the server issues always means what it was
//! issued for. And each encoder's output decodes back to its input, for any input the
//! encoder takes: arbitrary strings for UUID cursors, any `i64`, and any node id whose
//! type name is non-empty and has no `:` (a schema type name never has one; the id
//! after it may).

use fraiseql_core::runtime::relay::{
    decode_edge_cursor, decode_keyset_cursor, decode_node_id, decode_uuid_cursor,
    encode_edge_cursor, encode_keyset_cursor, encode_node_id, encode_uuid_cursor,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The input as a client-supplied cursor or id.
    let text = String::from_utf8_lossy(data);

    if let Some(cursor) = decode_keyset_cursor(&text) {
        assert_eq!(
            decode_keyset_cursor(&encode_keyset_cursor(&cursor)).as_ref(),
            Some(&cursor),
            "a keyset cursor does not survive re-encoding"
        );
    }
    if let Some(pk) = decode_edge_cursor(&text) {
        assert_eq!(decode_edge_cursor(&encode_edge_cursor(pk)), Some(pk));
    }
    if let Some(uuid) = decode_uuid_cursor(&text) {
        assert_eq!(decode_uuid_cursor(&encode_uuid_cursor(&uuid)).as_ref(), Some(&uuid));
    }
    if let Some((type_name, id)) = decode_node_id(&text) {
        assert_eq!(
            decode_node_id(&encode_node_id(&type_name, &id)),
            Some((type_name.clone(), id.clone())),
            "a node id does not survive re-encoding"
        );
    }

    // The input as what the server encodes.
    assert_eq!(decode_uuid_cursor(&encode_uuid_cursor(&text)).as_deref(), Some(&*text));
    if let Some(bytes) = data.get(..8) {
        let pk = i64::from_le_bytes(bytes.try_into().expect("8 bytes"));
        assert_eq!(decode_edge_cursor(&encode_edge_cursor(pk)), Some(pk));
    }
    if let Some((type_name, id)) = text.split_once('\u{0}') {
        if !type_name.is_empty() && !type_name.contains(':') && !id.is_empty() {
            assert_eq!(
                decode_node_id(&encode_node_id(type_name, id)),
                Some((type_name.to_string(), id.to_string())),
                "an issued node id decodes as another"
            );
        }
    }
});
