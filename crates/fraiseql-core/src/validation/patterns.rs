//! Shared regex patterns for field validation.
//!
//! Single source of truth for patterns used across both the synchronous
//! (`rich_scalars`) and async (`async_validators`) validation layers.

/// Email address pattern (RFC 5321 practical subset).
///
/// Matches `local-part@domain` where the domain consists of at least two
/// dot-separated labels (e.g. `example.com`).  Single-label domains such as
/// `user@localhost` are intentionally rejected.
pub const EMAIL: &str = concat!(
    r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+",  // local-part
    r"@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?", // first domain label
    r"(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+$", // one or more further labels
);
