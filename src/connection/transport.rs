//! Transport abstraction (TCP with optional TLS vs Unix socket)

use crate::Result;
use bytes::BytesMut;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};

/// TCP stream variant: plain or TLS-encrypted
pub enum TcpVariant {
    /// Plain TCP connection
    Plain(TcpStream),
    /// TLS-encrypted TCP connection
    Tls(tokio_rustls::client::TlsStream<TcpStream>),
}

impl std::fmt::Debug for TcpVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TcpVariant::Plain(_) => f.write_str("TcpVariant::Plain(TcpStream)"),
            TcpVariant::Tls(_) => f.write_str("TcpVariant::Tls(TlsStream)"),
        }
    }
}

impl TcpVariant {
    /// Write all bytes to the stream
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        match self {
            TcpVariant::Plain(stream) => stream.write_all(buf).await?,
            TcpVariant::Tls(stream) => stream.write_all(buf).await?,
        }
        Ok(())
    }

    /// Flush the stream
    pub async fn flush(&mut self) -> Result<()> {
        match self {
            TcpVariant::Plain(stream) => stream.flush().await?,
            TcpVariant::Tls(stream) => stream.flush().await?,
        }
        Ok(())
    }

    /// Read into buffer
    pub async fn read_buf(&mut self, buf: &mut BytesMut) -> Result<usize> {
        let n = match self {
            TcpVariant::Plain(stream) => stream.read_buf(buf).await?,
            TcpVariant::Tls(stream) => stream.read_buf(buf).await?,
        };
        Ok(n)
    }

    /// Shutdown the stream
    pub async fn shutdown(&mut self) -> Result<()> {
        match self {
            TcpVariant::Plain(stream) => stream.shutdown().await?,
            TcpVariant::Tls(stream) => stream.shutdown().await?,
        }
        Ok(())
    }
}

/// Transport layer abstraction
#[derive(Debug)]
pub enum Transport {
    /// TCP socket (plain or TLS)
    Tcp(TcpVariant),
    /// Unix domain socket
    Unix(UnixStream),
}

impl Transport {
    /// Connect via plain TCP
    pub async fn connect_tcp(host: &str, port: u16) -> Result<Self> {
        let stream = TcpStream::connect((host, port)).await?;
        Ok(Transport::Tcp(TcpVariant::Plain(stream)))
    }

    /// Connect via TLS-encrypted TCP
    pub async fn connect_tcp_tls(
        host: &str,
        port: u16,
        tls_config: &crate::connection::TlsConfig,
    ) -> Result<Self> {
        let tcp_stream = TcpStream::connect((host, port)).await?;

        // Parse server name for TLS handshake (SNI)
        let server_name = crate::connection::parse_server_name(host)?;
        let server_name = rustls_pki_types::ServerName::try_from(server_name)
            .map_err(|_| crate::Error::Config(format!("Invalid hostname for TLS: {}", host)))?;

        // Perform TLS handshake
        let client_config = tls_config.client_config();
        let tls_connector = tokio_rustls::TlsConnector::from(client_config);
        let tls_stream = tls_connector
            .connect(server_name, tcp_stream)
            .await
            .map_err(|e| crate::Error::Config(format!("TLS handshake failed: {}", e)))?;

        Ok(Transport::Tcp(TcpVariant::Tls(tls_stream)))
    }

    /// Connect via Unix socket
    pub async fn connect_unix(path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(path).await?;
        Ok(Transport::Unix(stream))
    }

    /// Write bytes to the transport
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        match self {
            Transport::Tcp(variant) => variant.write_all(buf).await?,
            Transport::Unix(stream) => stream.write_all(buf).await?,
        }
        Ok(())
    }

    /// Flush the transport
    pub async fn flush(&mut self) -> Result<()> {
        match self {
            Transport::Tcp(variant) => variant.flush().await?,
            Transport::Unix(stream) => stream.flush().await?,
        }
        Ok(())
    }

    /// Read bytes into buffer
    pub async fn read_buf(&mut self, buf: &mut BytesMut) -> Result<usize> {
        let n = match self {
            Transport::Tcp(variant) => variant.read_buf(buf).await?,
            Transport::Unix(stream) => stream.read_buf(buf).await?,
        };
        Ok(n)
    }

    /// Shutdown the transport
    pub async fn shutdown(&mut self) -> Result<()> {
        match self {
            Transport::Tcp(variant) => variant.shutdown().await?,
            Transport::Unix(stream) => stream.shutdown().await?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_connect_failure() {
        let result = Transport::connect_tcp("localhost", 9999).await;
        assert!(result.is_err());
    }
}
