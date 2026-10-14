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
