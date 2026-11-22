//! A loopback relay to PostgreSQL that can hold back the reply to a transaction start.
//!
//! A driver that starts a transaction sends `BEGIN` and waits for the server's reply.
//! A task cancelled during that wait is the case this relay makes reproducible: once
//! [`HeldReplies::arm`] is called, the next connection that sends a plain `BEGIN` has
//! every reply after it held back until [`HeldReplies::release`]. The server has run the
//! `BEGIN` by then, so that session is inside a transaction while its client still waits.
//!
//! The relay reads the client's bytes as they pass, so the connection must not be
//! encrypted: point the client at [`HeldReplies::port`] with TLS disabled.

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
};

/// A simple-protocol `Query` message whose text is exactly `BEGIN`: the tag `Q`, the
/// length 10 (itself, the text and its terminator), `BEGIN`, and the terminating NUL.
const BEGIN_QUERY: &[u8] = b"Q\0\0\0\x0aBEGIN\0";

/// The relay. Dropping it stops accepting connections; relayed ones run to their end.
pub struct HeldReplies {
    addr:     SocketAddr,
    armed:    Arc<AtomicBool>,
    held:     Arc<AtomicUsize>,
    released: watch::Sender<bool>,
    accept:   tokio::task::JoinHandle<()>,
}

impl HeldReplies {
    /// Relay loopback connections to the PostgreSQL server at `upstream` (`host:port`).
    ///
    /// # Panics
    ///
    /// When no loopback port can be bound.
    pub async fn start(upstream: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind a loopback port");
        let addr = listener.local_addr().expect("the bound address");
        let armed = Arc::new(AtomicBool::new(false));
        let held = Arc::new(AtomicUsize::new(0));
        let (released, _) = watch::channel(false);
        let accept = tokio::spawn({
            let armed = Arc::clone(&armed);
            let held = Arc::clone(&held);
            let released = released.clone();
            async move {
                while let Ok((client, _)) = listener.accept().await {
                    let Ok(server) = TcpStream::connect(&upstream).await else {
                        continue;
                    };
                    relay(
                        client,
                        server,
                        Arc::clone(&armed),
                        Arc::clone(&held),
                        released.subscribe(),
                    );
                }
            }
        });
        Self {
            addr,
            armed,
            held,
            released,
            accept,
        }
    }

    /// The loopback port clients connect to.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Hold the replies on the next connection that sends `BEGIN`, from that `BEGIN` on.
    pub fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Wait until a connection's `BEGIN` has reached the server with its reply held.
    ///
    /// # Panics
    ///
    /// When no `BEGIN` is held within `within`.
    pub async fn wait_until_held(&self, within: Duration) {
        let deadline = tokio::time::Instant::now() + within;
        while self.held.load(Ordering::SeqCst) == 0 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "no connection sent BEGIN within {within:?} of the relay being armed"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Let every held reply through, and stop holding.
    pub fn release(&self) {
        self.released.send_replace(true);
    }
}

impl Drop for HeldReplies {
    fn drop(&mut self) {
        self.accept.abort();
        self.release();
    }
}

fn relay(
    client: TcpStream,
    server: TcpStream,
    armed: Arc<AtomicBool>,
    held: Arc<AtomicUsize>,
    mut released: watch::Receiver<bool>,
) {
    let holding = Arc::new(AtomicBool::new(false));
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();

    let marking = Arc::clone(&holding);
    tokio::spawn(async move {
        let mut buf = vec![0_u8; 64 * 1024];
        // The bytes before this read, enough to find a `BEGIN` split across two reads.
        let mut carry: Vec<u8> = Vec::new();
        loop {
            let n = match client_read.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            carry.extend_from_slice(&buf[..n]);
            if !marking.load(Ordering::SeqCst)
                && carry.windows(BEGIN_QUERY.len()).any(|w| w == BEGIN_QUERY)
                && armed.swap(false, Ordering::SeqCst)
            {
                // Marked before the `BEGIN` is forwarded, so its reply cannot pass first.
                marking.store(true, Ordering::SeqCst);
                held.fetch_add(1, Ordering::SeqCst);
            }
            let keep = carry.len().saturating_sub(BEGIN_QUERY.len() - 1);
            carry.drain(..keep);
            if server_write.write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
        let _ = server_write.shutdown().await;
    });

    tokio::spawn(async move {
        let mut buf = vec![0_u8; 64 * 1024];
        loop {
            let n = match server_read.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if holding.load(Ordering::SeqCst) && released.wait_for(|r| *r).await.is_err() {
                break;
            }
            if client_write.write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
        let _ = client_write.shutdown().await;
    });
}
