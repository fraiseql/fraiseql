//! A PostgreSQL wire proxy that counts what a client costs (#1572, #1573).
//!
//! [`CountingProxy::start`] forwards connections to a database and counts, over all of them:
//!
//! - **round trips**: each time a client sends after its server last sent. A client that pipelines
//!   several statements before reading an answer pays one.
//! - **parses**: `Parse` messages, each a statement PostgreSQL plans.
//! - **executes**: `Execute` messages, each a prepared statement run (`BEGIN` and `COMMIT`, sent as
//!   simple queries, are not among them).
//!
//! Count one connection at a time (a pool of one) for exact numbers: two connections
//! interleave their flights. The proxy speaks plaintext only, so the URL it returns carries
//! `sslmode=disable`. It runs on threads of its own, under any async runtime or none.
//!
//! It can also fail like a network or a pooler does (#1577): [`sever`](CountingProxy::sever)
//! closes every connection and refuses new ones until [`restore`](CountingProxy::restore), and
//! [`drop_notifications`](CountingProxy::drop_notifications) swallows every
//! `NotificationResponse`, as a `LISTEN` behind `PgBouncer` in transaction mode receives nothing.

use std::{
    io::{Read as _, Write as _},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

/// What the proxy has counted since it started or was last [`reset`](Self::reset).
#[derive(Debug, Default)]
pub struct Wire {
    round_trips:  AtomicU64,
    parses:       AtomicU64,
    executes:     AtomicU64,
    server_spoke: AtomicBool,
}

impl Wire {
    /// Start counting from zero.
    pub fn reset(&self) {
        self.round_trips.store(0, Ordering::SeqCst);
        self.parses.store(0, Ordering::SeqCst);
        self.executes.store(0, Ordering::SeqCst);
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

    /// `Execute` messages counted.
    #[must_use]
    pub fn executes(&self) -> u64 {
        self.executes.load(Ordering::SeqCst)
    }
}

/// How the proxy fails, as its owner asks.
#[derive(Debug, Default)]
struct Faults {
    /// Refuse every new connection.
    refusing:           AtomicBool,
    /// Swallow every `NotificationResponse` the server sends.
    drop_notifications: AtomicBool,
    /// Both ends of every connection forwarded so far, to close them on `sever`.
    live:               Mutex<Vec<TcpStream>>,
}

/// A running proxy. It lives until the process ends.
#[derive(Debug)]
pub struct CountingProxy {
    url:    String,
    wire:   Arc<Wire>,
    faults: Arc<Faults>,
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
        let faults = Arc::new(Faults::default());
        let faulting = Arc::clone(&faults);
        thread::spawn(move || {
            for client in listener.incoming().flatten() {
                if faulting.refusing.load(Ordering::SeqCst) {
                    let _ = client.shutdown(Shutdown::Both);
                    continue;
                }
                let Ok(server) = TcpStream::connect(&upstream) else {
                    continue;
                };
                forward(client, server, &counted, &faulting);
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
            faults,
        }
    }

    /// Close every connection, and refuse new ones until [`restore`](Self::restore).
    ///
    /// # Panics
    ///
    /// If a thread panicked holding the list of connections.
    pub fn sever(&self) {
        self.faults.refusing.store(true, Ordering::SeqCst);
        for stream in self.faults.live.lock().expect("the connection list").drain(..) {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }

    /// Accept connections again.
    pub fn restore(&self) {
        self.faults.refusing.store(false, Ordering::SeqCst);
    }

    /// Swallow (`true`) or forward (`false`, the default) every `NotificationResponse`.
    pub fn drop_notifications(&self, drop: bool) {
        self.faults.drop_notifications.store(drop, Ordering::SeqCst);
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

fn forward(client: TcpStream, server: TcpStream, wire: &Arc<Wire>, faults: &Arc<Faults>) {
    let (Ok(mut client_rx), Ok(mut server_rx)) = (client.try_clone(), server.try_clone()) else {
        return;
    };
    if let (Ok(c), Ok(s)) = (client.try_clone(), server.try_clone()) {
        faults.live.lock().expect("the connection list").extend([c, s]);
    }
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
    let faults = Arc::clone(faults);
    thread::spawn(move || {
        let mut framer = BackendFramer::default();
        let mut buffer = vec![0_u8; 64 * 1024];
        while let Ok(n @ 1..) = server_rx.read(&mut buffer) {
            // Before forwarding: the client cannot answer bytes it has not received.
            down.server_spoke.store(true, Ordering::SeqCst);
            let drop = faults.drop_notifications.load(Ordering::SeqCst);
            let out = framer.forwardable(&buffer[..n], drop);
            if !out.is_empty() && client_tx.write_all(&out).is_err() {
                break;
            }
        }
        let _ = client_tx.shutdown(Shutdown::Write);
    });
}

/// Splits the server's byte stream into protocol messages, so one kind can be withheld.
#[derive(Default)]
struct BackendFramer {
    state:   BackendState,
    pending: Vec<u8>,
}

#[derive(Default, PartialEq, Eq)]
enum BackendState {
    /// Nothing received yet: the first byte may answer an `SSLRequest`.
    #[default]
    Start,
    /// Typed messages.
    Framed,
    /// TLS: opaque bytes, forwarded as they come.
    Opaque,
}

impl BackendFramer {
    /// `NotificationResponse`.
    const NOTIFICATION: u8 = b'A';

    /// The bytes of `bytes` (with what earlier calls held back) to forward now: every
    /// complete message, less notifications when `drop_notifications`.
    fn forwardable(&mut self, mut bytes: &[u8], drop_notifications: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        if self.state == BackendState::Start {
            match bytes.first() {
                // The answer to an `SSLRequest`: `S` begins TLS, `N` declines it.
                Some(b'S') => self.state = BackendState::Opaque,
                Some(b'N') => {
                    out.push(b'N');
                    bytes = &bytes[1..];
                    self.state = BackendState::Framed;
                },
                Some(_) => self.state = BackendState::Framed,
                None => return out,
            }
        }
        if self.state == BackendState::Opaque {
            out.extend_from_slice(bytes);
            return out;
        }
        self.pending.extend_from_slice(bytes);
        while self.pending.len() >= 5 {
            let declared = u32::from_be_bytes([
                self.pending[1],
                self.pending[2],
                self.pending[3],
                self.pending[4],
            ]);
            let length = usize::try_from(declared).expect("a u32 fits a usize") + 1;
            if self.pending.len() < length {
                break;
            }
            let message: Vec<u8> = self.pending.drain(..length).collect();
            if !(drop_notifications && message[0] == Self::NOTIFICATION) {
                out.extend_from_slice(&message);
            }
        }
        out
    }
}

/// Splits the client's byte stream into protocol messages, counting `Parse` and `Execute`.
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
                match self.pending[0] {
                    b'P' => wire.parses.fetch_add(1, Ordering::SeqCst),
                    b'E' => wire.executes.fetch_add(1, Ordering::SeqCst),
                    _ => 0,
                };
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
mod tests;
