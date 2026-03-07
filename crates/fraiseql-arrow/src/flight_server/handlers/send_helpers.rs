//! Helper functions for sending results through `tokio::sync::mpsc` channels in Flight handlers.
//!
//! Provides `send_err` and `send_ok` async helpers that wrap channel sends
//! and log a warning when the receiver has been dropped.

use tokio::sync::mpsc::Sender;
use tonic::Status;

/// Send an error status through the Flight response channel.
///
/// A `SendError` means the receiver was dropped (client disconnected) — the expected cleanup path.
/// Logs a warning if the channel is closed so callers can simply `return` without boilerplate.
pub(super) async fn send_err<T>(tx: &Sender<Result<T, Status>>, status: Status)
where
    T: Send,
{
    if tx.send(Err(status)).await.is_err() {
        tracing::warn!("send_err: receiver dropped (client disconnected)");
    }
}

/// Send a successful value through the Flight response channel.
///
/// A `SendError` means the receiver was dropped (client disconnected) — the expected cleanup path.
/// Returns `true` if the send succeeded, `false` if the channel was closed.
pub(super) async fn send_ok<T>(tx: &Sender<Result<T, Status>>, value: T) -> bool
where
    T: Send,
{
    if tx.send(Ok(value)).await.is_err() {
        tracing::warn!("send_ok: receiver dropped (client disconnected)");
        return false;
    }
    true
}
