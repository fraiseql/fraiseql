#![no_main]

use bytes::BytesMut;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut buf = BytesMut::from(data);
    let result = fraiseql_wire::protocol::decode_message(&mut buf);

    match result {
        Ok((_msg, consumed)) => {
            // Consumed bytes must not exceed input length
            assert!(
                consumed <= data.len(),
                "decode_message consumed {consumed} bytes from {}-byte input",
                data.len()
            );

            // Must consume at least the minimum message size (1 tag + 4 length)
            assert!(
                consumed >= 5,
                "decode_message consumed only {consumed} bytes (minimum valid message is 5)"
            );
        }
        Err(e) => {
            // Error messages must be non-empty
            let msg = e.to_string();
            assert!(!msg.is_empty(), "Decode error produced empty message");
        }
    }
});
