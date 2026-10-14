#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

fn message(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut m = vec![kind];
    m.extend_from_slice(&u32::try_from(body.len() + 4).unwrap().to_be_bytes());
    m.extend_from_slice(body);
    m
}

fn startup(code: u32, extra: &[u8]) -> Vec<u8> {
    let mut m = u32::try_from(8 + extra.len()).unwrap().to_be_bytes().to_vec();
    m.extend_from_slice(&code.to_be_bytes());
    m.extend_from_slice(extra);
    m
}

#[test]
fn messages_are_counted_after_startup_however_the_bytes_are_split() {
    let mut stream = startup(80_877_103, b"");
    stream.extend(startup(196_608, b"user\0u\0\0"));
    stream.extend(message(b'P', b"\0SELECT 1\0\0\0"));
    stream.extend(message(b'B', b"\0\0\0\0\0\0\0"));
    stream.extend(message(b'E', b"\0\0\0\0\0"));
    stream.extend(message(b'P', b"\0SELECT 2\0\0\0"));
    stream.extend(message(b'S', b""));
    for chunk in [1, 3, 7, stream.len()] {
        let (wire, mut framer) = (Wire::default(), FrontendFramer::default());
        for piece in stream.chunks(chunk) {
            framer.feed(piece, &wire);
        }
        assert_eq!((wire.parses(), wire.executes()), (2, 1), "split every {chunk} bytes");
    }
}

#[test]
fn the_url_points_at_the_proxy_in_plaintext() {
    let proxy = CountingProxy::start("postgresql://u:p@db.example:5433/test?sslmode=require");
    let url = proxy.url();
    assert!(url.starts_with("postgresql://u:p@127.0.0.1:"), "{url}");
    assert!(url.ends_with("/test?sslmode=disable"), "{url}");
}

#[test]
fn notifications_are_withheld_only_when_asked_however_the_bytes_are_split() {
    let mut stream = message(b'R', b"\0\0\0\0");
    stream.extend(message(b'A', b"\0\0\0\x07chan\0hello\0"));
    stream.extend(message(b'Z', b"I"));
    let withheld = [message(b'R', b"\0\0\0\0"), message(b'Z', b"I")].concat();
    for chunk in [1, 3, 7, stream.len()] {
        for (drop, expected) in [(false, &stream), (true, &withheld)] {
            let mut framer = BackendFramer::default();
            let out: Vec<u8> =
                stream.chunks(chunk).flat_map(|piece| framer.forwardable(piece, drop)).collect();
            assert_eq!(&out, expected, "split every {chunk} bytes, drop = {drop}");
        }
    }
}

#[test]
fn an_answer_to_an_ssl_request_is_forwarded() {
    let mut declined = vec![b'N'];
    declined.extend(message(b'A', b"\0\0\0\x07chan\0hi\0"));
    assert_eq!(BackendFramer::default().forwardable(&declined, true), vec![b'N']);
    let tls = [b'S', 0x16, 0x03, 0x01];
    assert_eq!(BackendFramer::default().forwardable(&tls, true), tls.to_vec());
}
