//! The authenticated principal every validator produces.
//!
//! [`AuthenticatedUser`] is what a verified bearer token becomes: the OIDC validator
//! builds one for an external provider's tokens and for the sessions this server's own
//! issuer signs. The claim readers in [`types`] flatten the nested claim shapes
//! providers emit.

pub mod types;

pub use types::AuthenticatedUser;

#[cfg(test)]
mod tests;
