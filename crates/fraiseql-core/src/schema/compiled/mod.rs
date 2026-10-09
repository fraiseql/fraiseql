//! Compiled schema types - pure Rust, no authoring-language references.
//!
//! These types represent GraphQL schemas after compilation from authoring languages.
//! All data is owned by Rust - no foreign object references.

pub mod argument;
pub mod directive;
mod fact_table_links;
mod federation_keys;
mod localized_indexes;
pub mod mutation;
pub mod query;
pub mod schema;
mod schema_domain;
mod schema_lookup;
mod schema_serde;
mod type_inject;
mod type_relationships;
mod type_roles;
mod type_scopes;
pub mod validation;

#[cfg(test)]
mod federation_keys_tests;
#[cfg(test)]
mod tests;

pub use argument::{ArgumentDefinition, AutoParams};
pub use directive::{DirectiveDefinition, DirectiveLocationKind};
pub use fact_table_links::fact_field;
pub use federation_keys::FederationKeyProblem;
pub use localized_indexes::LocalizedIndexAdvice;
pub use mutation::{InputStyle, MutationDefinition, MutationOperation};
pub use query::{CursorType, PaginationOrder, QueryDefinition};
pub use schema::{
    AppleSocialConfig, AuthClientConfig, CURRENT_FRAISEQL_VERSION, CompiledSchema,
    DiscordSocialConfig, FacebookSocialConfig, GitHubSocialConfig, GoogleSocialConfig,
    LocalAuthConfig, PkceClientConfig, ProducerVersion, SocialAuthConfig, SubscribableEntity,
};
pub use schema_serde::{canonicalize_json, content_hash_of};
pub use validation::is_safe_sql_identifier;
