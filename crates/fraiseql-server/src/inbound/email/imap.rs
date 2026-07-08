//! The IMAP transport edge: fetch raw messages above a UID watermark over TLS.
//!
//! This is the *only* impure, network-facing part of the email adapter — the
//! normalization above it ([`fraiseql_functions::normalize_email`]) and the
//! cursor arithmetic below it ([`super::cursor`]) are pure. The transport is
//! modelled behind [`MailboxFetcher`] so the [`source`](super::source) can be
//! driven by a fake in tests without a live server.
//!
//! Connections are short-lived: each poll opens an IMAPS (implicit-TLS)
//! connection, `SELECT`s the mailbox, `UID FETCH`es the new messages with
//! `BODY.PEEK[]` (peeking, so polling never sets `\Seen`), and drops the
//! connection. No IMAP-IDLE, no long-lived socket — *stateless with a cursor*.

use std::sync::Arc;

use fraiseql_functions::IngestError;
use futures::{StreamExt, future::BoxFuture};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, crypto::ring, pki_types::ServerName},
};

/// One raw message fetched from a mailbox, tagged with its IMAP `UID`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedMessage {
    /// The message's `UID` within the mailbox's current `UIDVALIDITY`.
    pub uid: u32,
    /// The raw RFC 5322 bytes (`BODY[]`).
    pub raw: Vec<u8>,
}

/// The result of one poll: the mailbox's current `UIDVALIDITY` and the fetched
/// messages (in server order, i.e. ascending `UID`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchBatch {
    /// The mailbox's `UIDVALIDITY` at fetch time — pairs with the cursor.
    pub uid_validity: u32,
    /// The fetched messages (at most the requested batch size).
    pub messages:     Vec<FetchedMessage>,
}

/// A transport that fetches new messages from a mailbox above a UID watermark.
///
/// Object-safe (it is used as `dyn MailboxFetcher`) and free of the `async_trait`
/// macro: the one method returns a boxed future explicitly.
pub trait MailboxFetcher: Send + Sync {
    /// Fetch up to `batch_size` messages newer than `stored`.
    ///
    /// The fetcher learns the mailbox's current `UIDVALIDITY` (on `SELECT`) and
    /// derives the fetch start from `stored` via [`super::cursor::fetch_start`],
    /// so a `UIDVALIDITY` reset re-scans from the beginning. The returned
    /// [`FetchBatch::uid_validity`] lets the worker advance the cursor correctly.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError`] on any connection, TLS, authentication, or
    /// protocol failure — a transient error the worker retries on the next poll
    /// without advancing the cursor.
    fn fetch(
        &self,
        stored: Option<super::cursor::Cursor>,
        batch_size: u32,
    ) -> BoxFuture<'_, Result<FetchBatch, IngestError>>;
}

/// A live poll-IMAP fetcher over implicit TLS.
pub struct ImapMailboxFetcher {
    host:      String,
    port:      u16,
    username:  String,
    password:  String,
    mailbox:   String,
    connector: TlsConnector,
}

impl ImapMailboxFetcher {
    /// Build a fetcher for one mailbox.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError`] if the rustls client configuration cannot be built.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
        mailbox: impl Into<String>,
    ) -> Result<Self, IngestError> {
        Ok(Self {
            host: host.into(),
            port,
            username: username.into(),
            password: password.into(),
            mailbox: mailbox.into(),
            connector: tls_connector()?,
        })
    }

    /// Open a connection, fetch the batch, and drop the connection.
    async fn fetch_batch(
        &self,
        stored: Option<super::cursor::Cursor>,
        batch_size: u32,
    ) -> Result<FetchBatch, IngestError> {
        let tcp = TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|error| IngestError::new(format!("imap connect {}: {error}", self.host)))?;
        let server_name = ServerName::try_from(self.host.clone())
            .map_err(|error| IngestError::new(format!("imap server name: {error}")))?;
        let tls = self
            .connector
            .connect(server_name, tcp)
            .await
            .map_err(|error| IngestError::new(format!("imap TLS handshake: {error}")))?;

        // async-imap's `runtime-tokio` mode speaks tokio's AsyncRead/AsyncWrite, so
        // the tokio-rustls stream is handed over directly — no compat shim.
        let client = async_imap::Client::new(tls);
        let mut session = client
            .login(&self.username, &self.password)
            .await
            .map_err(|(error, _client)| IngestError::new(format!("imap login: {error}")))?;

        let batch = self.fetch_selected(&mut session, stored, batch_size).await;

        // Stateless per poll: drop the session (closing the connection) whether or
        // not the fetch succeeded. A best-effort logout would risk fighting the
        // half-drained fetch stream, so we simply let the socket close.
        drop(session);
        batch
    }

    /// `SELECT` the mailbox and `UID FETCH` the new messages.
    async fn fetch_selected<T>(
        &self,
        session: &mut async_imap::Session<T>,
        stored: Option<super::cursor::Cursor>,
        batch_size: u32,
    ) -> Result<FetchBatch, IngestError>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug,
    {
        let mailbox = session
            .select(&self.mailbox)
            .await
            .map_err(|error| IngestError::new(format!("imap SELECT {}: {error}", self.mailbox)))?;
        let uid_validity = mailbox.uid_validity.ok_or_else(|| {
            IngestError::new("imap SELECT did not report UIDVALIDITY; cannot cursor safely")
        })?;
        // Derive the fetch start under the mailbox's *current* UIDVALIDITY, so a
        // reset (or a first poll) re-scans from UID 1.
        let fetch_start = super::cursor::fetch_start(stored, uid_validity);

        // `start:*` returns every message from `start` to the highest UID. Servers
        // stream FETCH results in ascending UID order, so taking the first
        // `batch_size` yields the oldest new messages; the remainder is picked up
        // on the next poll once the cursor advances.
        let query = format!("{fetch_start}:*");
        let mut stream = session
            .uid_fetch(query, "(UID BODY.PEEK[])")
            .await
            .map_err(|error| IngestError::new(format!("imap UID FETCH: {error}")))?;

        let mut messages = Vec::new();
        while let Some(item) = stream.next().await {
            let fetch =
                item.map_err(|error| IngestError::new(format!("imap FETCH item: {error}")))?;
            let (Some(uid), Some(body)) = (fetch.uid, fetch.body()) else {
                continue;
            };
            messages.push(FetchedMessage {
                uid,
                raw: body.to_vec(),
            });
            if messages.len() >= batch_size as usize {
                break;
            }
        }
        drop(stream);

        Ok(FetchBatch {
            uid_validity,
            messages,
        })
    }
}

impl MailboxFetcher for ImapMailboxFetcher {
    fn fetch(
        &self,
        stored: Option<super::cursor::Cursor>,
        batch_size: u32,
    ) -> BoxFuture<'_, Result<FetchBatch, IngestError>> {
        Box::pin(self.fetch_batch(stored, batch_size))
    }
}

/// Build a rustls TLS connector trusting the webpki (Mozilla) root store.
///
/// The `ring` provider is selected explicitly so the connector never depends on
/// which process-default rustls provider happens to be installed.
fn tls_connector() -> Result<TlsConnector, IngestError> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|error| IngestError::new(format!("imap rustls config: {error}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

#[cfg(test)]
mod tests;
