//! TLS listener utilities for accepting both plain and encrypted connections.

use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// TLS listener configuration.
pub struct TlsListenerConfig {
    /// The TLS acceptor (for TLS connections).
    pub acceptor: Option<TlsAcceptor>,
}

impl TlsListenerConfig {
    /// Create a new plain listener config (no TLS).
    pub const fn plain() -> Self {
        Self { acceptor: None }
    }

    /// Create a new TLS listener config.
    pub const fn tls(acceptor: TlsAcceptor) -> Self {
        Self {
            acceptor: Some(acceptor),
        }
    }

    /// Check if TLS is enabled.
    pub const fn is_tls_enabled(&self) -> bool {
        self.acceptor.is_some()
    }
}

/// Connection type for either plain TCP or TLS.
pub enum AcceptedConnection {
    /// Plain TCP connection.
    Plain(tokio::net::TcpStream),
    /// TLS connection.
    Tls(Box<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>),
}

impl AcceptedConnection {
    /// Get the remote socket address.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if the underlying stream's peer address cannot be retrieved.
    pub fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        match self {
            Self::Plain(stream) => stream.peer_addr(),
            Self::Tls(stream) => stream.get_ref().0.peer_addr(),
        }
    }
}

/// Accept the next connection with optional TLS wrapping.
///
/// # Errors
///
/// Returns `std::io::Error` if the TCP listener fails to accept a connection or the TLS handshake fails.
pub async fn accept_connection(
    listener: &TcpListener,
    config: &TlsListenerConfig,
) -> std::io::Result<(AcceptedConnection, std::net::SocketAddr)> {
    let (stream, addr) = listener.accept().await?;

    match &config.acceptor {
        None => {
            // Plain connection
            Ok((AcceptedConnection::Plain(stream), addr))
        },
        Some(acceptor) => {
            // TLS connection
            match acceptor.accept(stream).await {
                Ok(tls_stream) => Ok((AcceptedConnection::Tls(Box::new(tls_stream)), addr)),
                Err(e) => Err(std::io::Error::other(e)),
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_config() {
        let config = TlsListenerConfig::plain();
        assert!(!config.is_tls_enabled());
    }
}
