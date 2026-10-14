//! A PostgreSQL wire proxy that counts what a client costs (#1572, #1573).
//!
//! [`CountingProxy::start`] forwards connections to a database and counts, over all of them:
//!
//! - **round trips**: each time a client sends after its server last sent. A client that pipelines
//!   several statements before reading an answer pays one.
//! - **parses**: `Parse` messages, each a statement PostgreSQL plans.
//!
//! Count one connection at a time (a pool of one) for exact numbers: two connections
//! interleave their flights. The proxy speaks plaintext only, so the URL it returns carries
//! `sslmode=disable`. It runs on threads of its own, under any async runtime or none.

use std::{
    io::{Read as _, Write as _},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

/// What the proxy has counted since it started or was last [`reset`](Self::reset).
#[derive(Debug, Default)]
pub struct Wire {
    round_trips:  AtomicU64,
    parses:       AtomicU64,
    server_spoke: AtomicBool,
}

impl Wire {
    /// Start counting from zero.
    pub fn reset(&self) {
        self.round_trips.store(0, Ordering::SeqCst);
        self.parses.store(0, Ordering::SeqCst);
    }

    /// Round trips counted.
    #[must_use]
    pub fn round_trips(&self) -> u64 {
        self.round_trips.load(Ordering::SeqCst)
    }

    /// `Parse` messages counted.
    #[must_use]
    pub fn parses(&self) -> u64 {
        self.parses.load(Ordering::SeqCst)
    }
}

/// A running proxy. It lives until the process ends.
#[derive(Debug)]
pub struct CountingProxy {
    url:  String,
    wire: Arc<Wire>,
}

impl CountingProxy {
    /// Forward to the database `upstream_url` (`postgresql://user:pass@host:port/db…`) names.
    ///
    /// # Panics
    ///
    /// If the URL has no `host:port`, or the proxy cannot listen on a loopback port.
    #[must_use]
    pub fn start(upstream_url: &str) -> Self {
        let (scheme, rest) = upstream_url.split_once("://").expect("a URL with a scheme");
        let (credentials, rest) = rest.rsplit_once('@').map_or(("", rest), |(c, r)| (c, r));
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let upstream = if authority.contains(':') {
            authority.to_string()
        } else {
            format!("{authority}:5432")
        };
        let query: Vec<&str> = query
            .split('&')
            .filter(|kv| !kv.is_empty() && !kv.starts_with("sslmode="))
            .chain(["sslmode=disable"])
            .collect();

        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound port").port();
        let wire = Arc::new(Wire::default());
        let counted = Arc::clone(&wire);
        thread::spawn(move || {
            for client in listener.incoming().flatten() {
                let Ok(server) = TcpStream::connect(&upstream) else {
                    continue;
                };
                forward(client, server, &counted);
            }
        });

        let at = if credentials.is_empty() {
            String::new()
        } else {
            format!("{credentials}@")
        };
        Self {
            url: format!("{scheme}://{at}127.0.0.1:{port}/{path}?{}", query.join("&")),
            wire,
        }
    }

    /// The URL to connect through the proxy.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The counts.
    #[must_use]
    pub fn wire(&self) -> &Wire {
        &self.wire
    }
}

fn forward(client: TcpStream, server: TcpStream, wire: &Arc<Wire>) {
    let (Ok(mut client_rx), Ok(mut server_rx)) = (client.try_clone(), server.try_clone()) else {
        return;
    };
    let (mut client_tx, mut server_tx) = (client, server);
    let up = Arc::clone(wire);
    thread::spawn(move || {
        let mut framer = FrontendFramer::default();
        let mut buffer = vec![0_u8; 64 * 1024];
        while let Ok(n @ 1..) = client_rx.read(&mut buffer) {
            if up.server_spoke.swap(false, Ordering::SeqCst) {
                up.round_trips.fetch_add(1, Ordering::SeqCst);
            }
            framer.feed(&buffer[..n], &up);
            if server_tx.write_all(&buffer[..n]).is_err() {
                break;
            }
        }
        let _ = server_tx.shutdown(Shutdown::Write);
    });
    let down = Arc::clone(wire);
    thread::spawn(move || {
        let mut buffer = vec![0_u8; 64 * 1024];
        while let Ok(n @ 1..) = server_rx.read(&mut buffer) {
            // Before forwarding: the client cannot answer bytes it has not received.
            down.server_spoke.store(true, Ordering::SeqCst);
            if client_tx.write_all(&buffer[..n]).is_err() {
                break;
            }
        }
        let _ = client_tx.shutdown(Shutdown::Write);
    });
}

/// Splits the client's byte stream into protocol messages, counting `Parse`.
#[derive(Default)]
struct FrontendFramer {
    started: bool,
    pending: Vec<u8>,
}

impl FrontendFramer {
    /// `SSLRequest` and `GSSENCRequest` precede the startup message, untyped like it.
    const NEGOTIATION_CODES: [u32; 2] = [80_877_103, 80_877_104];

    fn feed(&mut self, bytes: &[u8], wire: &Wire) {
        self.pending.extend_from_slice(bytes);
        loop {
            let at = usize::from(self.started);
            if self.pending.len() < at + 4 {
                return;
            }
            let declared = u32::from_be_bytes([
                self.pending[at],
                self.pending[at + 1],
                self.pending[at + 2],
                self.pending[at + 3],
            ]);
            let length = usize::try_from(declared).expect("a u32 fits a usize") + at;
            if self.pending.len() < length {
                return;
            }
            if self.started {
                if self.pending[0] == b'P' {
                    wire.parses.fetch_add(1, Ordering::SeqCst);
                }
            } else {
                let code = u32::from_be_bytes([
                    self.pending[4],
                    self.pending[5],
                    self.pending[6],
                    self.pending[7],
                ]);
                self.started = !Self::NEGOTIATION_CODES.contains(&code);
            }
            self.pending.drain(..length);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
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
    fn parses_are_counted_after_startup_however_the_bytes_are_split() {
        let mut stream = startup(80_877_103, b"");
        stream.extend(startup(196_608, b"user\0u\0\0"));
        stream.extend(message(b'P', b"\0SELECT 1\0\0\0"));
        stream.extend(message(b'B', b"\0\0\0\0\0\0\0"));
        stream.extend(message(b'P', b"\0SELECT 2\0\0\0"));
        stream.extend(message(b'S', b""));
        for chunk in [1, 3, 7, stream.len()] {
            let (wire, mut framer) = (Wire::default(), FrontendFramer::default());
            for piece in stream.chunks(chunk) {
                framer.feed(piece, &wire);
            }
            assert_eq!(wire.parses(), 2, "split every {chunk} bytes");
        }
    }

    #[test]
    fn the_url_points_at_the_proxy_in_plaintext() {
        let proxy = CountingProxy::start("postgresql://u:p@db.example:5433/test?sslmode=require");
        let url = proxy.url();
        assert!(url.starts_with("postgresql://u:p@127.0.0.1:"), "{url}");
        assert!(url.ends_with("/test?sslmode=disable"), "{url}");
    }
}
