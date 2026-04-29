//! In-process test server that binds to an ephemeral port.
//!
//! # Usage
//!
//! ```no_run
//! let server = TestServer::start(schema, adapter).await;
//! // server.url is "http://127.0.0.1:{port}"
//! // Server shuts down when TestServer is dropped.
//! ```

use std::sync::Arc;

use fraiseql_core::{db::traits::{DatabaseAdapter, SupportsMutations}, schema::CompiledSchema};
use fraiseql_server::{Server, server_config::ServerConfig};
use tokio::{net::TcpListener, sync::oneshot};

/// An in-process HTTP server bound to an ephemeral port for integration testing.
pub struct TestServer {
    /// Base URL of the running server (e.g., `"http://127.0.0.1:12345"`).
    pub url:   String,
    /// Bound port.
    pub port:  u16,
    // Dropping this sender triggers graceful shutdown via the oneshot channel.
    _shutdown: oneshot::Sender<()>,
}

impl TestServer {
    /// Start a server with the given schema and database adapter.
    ///
    /// Binds to `127.0.0.1:0` (OS-assigned ephemeral port), spawns the server as
    /// a background Tokio task, and waits briefly for it to be ready.
    ///
    /// # Panics
    ///
    /// Panics if the listener cannot be bound or the server fails to start.
    pub async fn start<A>(schema: CompiledSchema, adapter: Arc<A>) -> Self
    where
        A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind to ephemeral port");
        let port = listener.local_addr().expect("local addr").port();

        let config = ServerConfig::default();
        let server = Server::new(config, schema, adapter, None).await.expect("Server::new");

        let (tx, rx) = oneshot::channel::<()>();

        tokio::spawn(async move {
            server
                .serve_on_listener(listener, async {
                    let _ = rx.await; // intentional
                })
                .await
                .expect("server task failed");
        });

        // Give the Tokio task time to enter the accept loop.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        Self {
            url: format!("http://127.0.0.1:{port}"),
            port,
            _shutdown: tx,
        }
    }
}
