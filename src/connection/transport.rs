//! Transport abstraction (TCP vs Unix socket)

use crate::Result;
use bytes::BytesMut;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};

/// Transport layer abstraction
#[derive(Debug)]
pub enum Transport {
    /// TCP socket
    Tcp(TcpStream),
    /// Unix domain socket
    Unix(UnixStream),
}

impl Transport {
    /// Connect via TCP
    pub async fn connect_tcp(host: &str, port: u16) -> Result<Self> {
        let stream = TcpStream::connect((host, port)).await?;
        Ok(Transport::Tcp(stream))
    }

    /// Connect via Unix socket
    pub async fn connect_unix(path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(path).await?;
        Ok(Transport::Unix(stream))
    }

    /// Write bytes to the transport
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        match self {
            Transport::Tcp(stream) => stream.write_all(buf).await?,
            Transport::Unix(stream) => stream.write_all(buf).await?,
        }
        Ok(())
    }

    /// Flush the transport
    pub async fn flush(&mut self) -> Result<()> {
        match self {
            Transport::Tcp(stream) => stream.flush().await?,
            Transport::Unix(stream) => stream.flush().await?,
        }
        Ok(())
    }

    /// Read bytes into buffer
    pub async fn read_buf(&mut self, buf: &mut BytesMut) -> Result<usize> {
        let n = match self {
            Transport::Tcp(stream) => stream.read_buf(buf).await?,
            Transport::Unix(stream) => stream.read_buf(buf).await?,
        };
        Ok(n)
    }

    /// Shutdown the transport
    pub async fn shutdown(&mut self) -> Result<()> {
        match self {
            Transport::Tcp(stream) => stream.shutdown().await?,
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
