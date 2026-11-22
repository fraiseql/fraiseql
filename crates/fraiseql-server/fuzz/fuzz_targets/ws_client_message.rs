#![no_main]
//! A WebSocket client's frames, through what the subscription route does with them
//! before any operation runs: the `Sec-WebSocket-Protocol` negotiation, the codec's
//! decode under both protocols, the message type and the subscribe payload. Then the
//! one client value the server echoes, the operation id, through the codec's encode.
//!
//! The input's first line is the protocol header; the rest is the frame.
//!
//! Beyond "no panic":
//! - The two protocols decode the same frame to the same id and payload, and differ in type only by
//!   the legacy renames (`start` to `subscribe`, `stop` to `complete`).
//! - An error frame echoing the client's id encodes, under each protocol, to JSON whose `id` is
//!   that id: a client-chosen id cannot break the frame the server sends back.

use fraiseql_core::runtime::subscription::protocol::{GraphQLError, ServerMessage};
use fraiseql_server::subscriptions::{ProtocolCodec, WsProtocol};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let (header, frame) = text.split_once('\n').unwrap_or(("", &text));
    let _ = WsProtocol::from_header(Some(header));

    let modern = ProtocolCodec::new(WsProtocol::GraphqlTransportWs);
    let legacy = ProtocolCodec::new(WsProtocol::GraphqlWs);
    let (Ok(m), Ok(l)) = (modern.decode(frame), legacy.decode(frame)) else {
        assert_eq!(
            modern.decode(frame).is_ok(),
            legacy.decode(frame).is_ok(),
            "one protocol accepts a frame the other refuses"
        );
        return;
    };

    assert_eq!(m.id, l.id);
    assert_eq!(m.payload, l.payload);
    let renamed = match m.message_type.as_str() {
        "start" => "subscribe",
        "stop" => "complete",
        other => other,
    };
    assert_eq!(l.message_type, renamed, "the legacy decode renamed a type it should not");
    let _ = m.parsed_type();
    let _ = m.connection_params();
    let _ = m.subscription_payload();

    if let Some(id) = m.id {
        let reply = ServerMessage::error(id.clone(), vec![GraphQLError::new("refused")]);
        for codec in [&modern, &legacy] {
            let json = codec
                .encode(&reply)
                .expect("an error frame encodes")
                .expect("an error frame is never suppressed");
            let value: serde_json::Value =
                serde_json::from_str(&json).expect("the server sent invalid JSON");
            assert_eq!(value["id"].as_str(), Some(id.as_str()), "the echoed id changed");
        }
    }
});
