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

fn notification(channel: &str) -> Vec<u8> {
    message(b'A', &[b"\0\0\0\x07", channel.as_bytes(), b"\0hello\0"].concat())
}

#[test]
fn notifications_are_withheld_only_when_asked_however_the_bytes_are_split() {
    let mut stream = message(b'R', b"\0\0\0\0");
    stream.extend(notification("chan"));
    stream.extend(message(b'Z', b"I"));
    let withheld = [message(b'R', b"\0\0\0\0"), message(b'Z', b"I")].concat();
    for chunk in [1, 3, 7, stream.len()] {
        for (withhold, expected) in [(Withheld::Nothing, &stream), (Withheld::Every, &withheld)] {
            let mut framer = BackendFramer::default();
            let out: Vec<u8> = stream
                .chunks(chunk)
                .flat_map(|piece| framer.forwardable(piece, &withhold))
                .collect();
            assert_eq!(&out, expected, "split every {chunk} bytes, {withhold:?}");
        }
    }
}

#[test]
fn only_the_named_channels_notifications_are_withheld_however_the_bytes_are_split() {
    let stream = [
        notification("probe"),
        notification("feed"),
        notification("feed_other"),
        message(b'Z', b"I"),
    ]
    .concat();
    let expected = [
        notification("probe"),
        notification("feed_other"),
        message(b'Z', b"I"),
    ]
    .concat();
    for chunk in [1, 3, 7, stream.len()] {
        let mut framer = BackendFramer::default();
        let withhold = Withheld::Channel(b"feed".to_vec());
        let out: Vec<u8> = stream
            .chunks(chunk)
            .flat_map(|piece| framer.forwardable(piece, &withhold))
            .collect();
        assert_eq!(out, expected, "split every {chunk} bytes");
    }
}

#[test]
fn an_answer_to_an_ssl_request_is_forwarded() {
    let mut declined = vec![b'N'];
    declined.extend(notification("chan"));
    assert_eq!(BackendFramer::default().forwardable(&declined, &Withheld::Every), vec![b'N']);
    let tls = [b'S', 0x16, 0x03, 0x01];
    assert_eq!(BackendFramer::default().forwardable(&tls, &Withheld::Every), tls.to_vec());
}

#[test]
fn every_connection_forwarded_is_counted() {
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = upstream.local_addr().unwrap().port();
    thread::spawn(move || {
        let held: Vec<TcpStream> = upstream.incoming().flatten().collect();
        drop(held);
    });
    let proxy = CountingProxy::start(&format!("postgresql://u:p@127.0.0.1:{port}/test"));
    let at = proxy.url().split_once('@').unwrap().1.split_once('/').unwrap().0.to_string();
    let _clients: Vec<TcpStream> = (0..2).map(|_| TcpStream::connect(&at).unwrap()).collect();
    let start = std::time::Instant::now();
    while proxy.connections() < 2 {
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "{}", proxy.connections());
        thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(proxy.connections(), 2);
}
