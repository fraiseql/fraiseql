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

use fraiseql_core::{db::traits::DatabaseAdapter, schema::CompiledSchema};
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
        A: DatabaseAdapter + Clone + Send + Sync + 'static,
    {
        // Boxed so callers of `start` do not await a ~19 KiB future on the stack:
        // delegating to `start_with_config` nests the `Server::new` future inside this
        // one, and `clippy::large_futures` (pedantic, denied) rejects the result at every
        // call site otherwise.
        Box::pin(Self::start_with_config(ServerConfig::default(), schema, adapter)).await
    }

    /// Start a server with an explicit [`ServerConfig`].
    ///
    /// Needed by suites that must exercise a configured subsystem through the
    /// real mount — notably authentication, which is attached during
    /// `build_router` and is therefore invisible to any test that constructs a
    /// sub-router directly.
    ///
    /// # Panics
    ///
    /// Panics if the listener cannot be bound or the server fails to start.
    pub async fn start_with_config<A>(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
    ) -> Self
    where
        A: DatabaseAdapter + Clone + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind to ephemeral port");
        let port = listener.local_addr().expect("local addr").port();

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

    /// Start a server whose REST transport includes its **write** half, exactly as
    /// the binary's PostgreSQL boot path does.
    ///
    /// This is a separate entry point rather than a flag because
    /// `SupportsMutations` is not a bound on `Server<A>`'s lifecycle — the write
    /// router can only be installed where the concrete adapter is known. A test that
    /// short-circuited this by merging `rest_router` itself would be exercising a
    /// router the binary never serves, which is exactly how #812 (no auth on the REST
    /// mount) and #865 (no write routes at all) each survived two releases.
    ///
    /// # Panics
    ///
    /// Panics if the listener cannot be bound or the server fails to start.
    #[cfg(feature = "rest")]
    pub async fn start_with_rest_writes<A>(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
    ) -> Self
    where
        A: DatabaseAdapter
            + fraiseql_core::db::traits::SupportsMutations
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind to ephemeral port");
        let port = listener.local_addr().expect("local addr").port();

        let server = Box::pin(Server::new(config, schema, adapter, None))
            .await
            .expect("Server::new")
            .with_rest_write_surface();

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
