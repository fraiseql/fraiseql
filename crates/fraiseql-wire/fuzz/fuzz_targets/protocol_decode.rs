#![no_main]

use bytes::BytesMut;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // decode_message must never panic on arbitrary byte buffers
    let mut buf = BytesMut::from(data);
    let _ = fraiseql_wire::protocol::decode_message(&mut buf);
});
