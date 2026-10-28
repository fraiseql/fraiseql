//! Provider-specific OAuth / OIDC implementations.
//!
//! Each sub-module translates one identity provider's user-info format into FraiseQL's
//! generic [`crate::provider::UserInfo`]. None of them derives a role or an organisation:
//! the identity is the provider's subject and email, nothing more.
//!
//! Supported providers: Apple, Discord, Facebook, GitHub and Google, each selected by its
//! `[auth.social.<provider>]` block.

pub mod apple;
pub mod discord;
pub mod facebook;
pub mod github;
pub mod google;

pub use apple::AppleOAuth;
pub use discord::DiscordOAuth;
pub use facebook::FacebookOAuth;
pub use github::GitHubOAuth;
pub use google::GoogleOAuth;

#[cfg(test)]
mod tests;
