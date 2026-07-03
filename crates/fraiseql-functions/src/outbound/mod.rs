//! Per-user outbound-send policy — the sender-identity rule for any paired
//! outbound email.
//!
//! Sending stays per-user (a banked design input): a paired outbound email — a
//! sequence step, a follow-up, an auto-acknowledgement — goes *from the connected
//! user's verified address*, never from a shared or default mailbox. This module
//! encodes that rule as a pure, fail-loud policy: [`resolve_sender_identity`]
//! reads the connected user's verified sending address out of the host
//! [`auth_context`](crate::HostContext::auth_context) and refuses, loudly, when
//! there is none.
//!
//! The identity is host-owned by construction — it comes from the authenticated
//! context the server populates, not from guest input — so a function cannot
//! choose to send from another address. That is the enforcement: the only sending
//! identity a function can obtain is its own connected user's, and the absence of
//! one is an error rather than a silent fall-back to a shared mailbox.
//!
//! A first-class `send_email` host op that injects the bound `from` and a concrete
//! SMTP / provider transport are the Phase 06 promotion of this policy (see
//! `docs/architecture/native-runtime-ergonomics.md`); this module ships the
//! enforceable rule that op will call, and the reference workload
//! `examples/native-functions/follow-up-email.ts` mirrors it in `TypeScript`.

use serde_json::Value;

/// The connected user's verified sending identity — the only `from` a paired
/// outbound email may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderIdentity {
    /// The verified sending address (the message `from`).
    pub address:      String,
    /// The user's display name, if the auth context carries one.
    pub display_name: Option<String>,
}

/// A refusal to send: the per-user policy could not be satisfied.
///
/// The policy fails loud rather than fabricating a sender, so a misconfiguration
/// can never cause a send from the wrong — or a shared — mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendPolicyError {
    /// Human-readable reason the send is refused.
    pub message: String,
}

impl SendPolicyError {
    /// Build a refusal from a reason.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SendPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SendPolicyError {}

/// Resolve the connected user's verified sending identity from a host auth
/// context.
///
/// The address is taken from the `email` field the host populates from the
/// authenticated identity. It must be a non-empty, plausibly-addressable string
/// (containing an `@`); anything else — a missing, blank, or malformed value —
/// is a refusal, because a paired outbound email must be sent from the connected
/// user's address and never from a shared or default mailbox.
///
/// # Errors
///
/// Returns [`SendPolicyError`] when the auth context carries no usable verified
/// sending address.
pub fn resolve_sender_identity(auth_context: &Value) -> Result<SenderIdentity, SendPolicyError> {
    let address = auth_context
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| value.contains('@') && !value.contains(char::is_whitespace))
        .ok_or_else(|| {
            SendPolicyError::new(
                "refusing to send: the authenticated user has no verified sending address; a \
                 paired outbound email must be sent from the connected user's address, never a \
                 shared or default mailbox",
            )
        })?;

    let display_name = auth_context
        .get("display_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    Ok(SenderIdentity {
        address: address.to_string(),
        display_name,
    })
}

#[cfg(test)]
mod tests;
