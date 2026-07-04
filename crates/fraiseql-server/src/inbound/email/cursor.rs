//! The per-mailbox UID watermark — the only state a poll-IMAP source keeps.
//!
//! IMAP assigns each message a `UID` that is monotonically increasing and stable
//! within a `(mailbox, UIDVALIDITY)` pair. The cursor records the `UIDVALIDITY`
//! it was taken under and the highest `UID` already ingested. On each poll the
//! source fetches everything above the watermark; if the server reports a new
//! `UIDVALIDITY` the UID space has been reset underneath us, so the watermark is
//! discarded and the mailbox is re-scanned from the start — the spine's
//! `Message-ID` dedup makes the re-scan harmless.
//!
//! This module is the pure, unit-tested arithmetic of that scheme; the
//! [`store`](super::store) module persists it and the [`worker`](super::worker)
//! module drives it.

/// A UID watermark for one mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// The IMAP `UIDVALIDITY` this watermark was taken under.
    pub uid_validity: u32,
    /// The highest `UID` already ingested under that `UIDVALIDITY`.
    pub last_uid:     u32,
}

impl Cursor {
    /// Build a cursor from its parts.
    #[must_use]
    pub const fn new(uid_validity: u32, last_uid: u32) -> Self {
        Self {
            uid_validity,
            last_uid,
        }
    }
}

/// The effective highest-ingested `UID` for the mailbox's *current*
/// `UIDVALIDITY`.
///
/// A stored cursor only counts while its `UIDVALIDITY` still matches; a mismatch
/// (or no cursor at all) means the UID space was reset, so the effective
/// watermark is `0` and every message is new.
#[must_use]
pub const fn effective_last_uid(stored: Option<Cursor>, current_uid_validity: u32) -> u32 {
    match stored {
        Some(cursor) if cursor.uid_validity == current_uid_validity => cursor.last_uid,
        _ => 0,
    }
}

/// The first `UID` (inclusive) to FETCH from — the caller fetches `start:*`.
///
/// One past the effective watermark, so a fresh or reset mailbox starts at `1`.
/// Saturating: a mailbox already at `u32::MAX` re-fetches the last message rather
/// than wrapping to `0` (the spine dedup absorbs it).
#[must_use]
pub const fn fetch_start(stored: Option<Cursor>, current_uid_validity: u32) -> u32 {
    effective_last_uid(stored, current_uid_validity).saturating_add(1)
}

/// Whether a fetched `UID` is genuinely new relative to the effective watermark.
///
/// Guards against the IMAP `n:*` quirk, where a server returns the single highest
/// message when `n` exceeds the greatest UID, by re-checking `uid > watermark`
/// after the fetch.
#[must_use]
pub const fn is_new(uid: u32, effective_last_uid: u32) -> bool {
    uid > effective_last_uid
}

/// The cursor to persist after committing messages up to `highest_committed_uid`.
///
/// Always keyed to the current `UIDVALIDITY`; the `UID` never moves backwards, so
/// a batch that committed nothing new leaves the watermark where it was.
#[must_use]
pub const fn advanced(
    current_uid_validity: u32,
    effective_last_uid: u32,
    highest_committed_uid: u32,
) -> Cursor {
    // `u32::max` is not const-stable on the MSRV, so branch explicitly.
    let last = if highest_committed_uid > effective_last_uid {
        highest_committed_uid
    } else {
        effective_last_uid
    };
    Cursor::new(current_uid_validity, last)
}

#[cfg(test)]
mod tests;
