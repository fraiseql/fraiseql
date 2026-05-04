//! Supporting modules for executor runners.
//!
//! Pure helper functions shared across runners — no `&self`, no `DatabaseAdapter`
//! type parameter required.

pub(super) mod security;
