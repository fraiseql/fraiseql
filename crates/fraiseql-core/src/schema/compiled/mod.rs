//! Compiled schema types - pure Rust, no authoring-language references.
//!
//! These types represent GraphQL schemas after compilation from authoring languages.
//! All data is owned by Rust - no foreign object references.

pub mod argument;
mod auth_email;
pub mod directive;
mod fact_table_links;
mod fact_table_scope;
mod federation_keys;
mod localized_indexes;
pub mod mutation;
mod otp_allowed_query;
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
mod default_literal_tests;
#[cfg(test)]
mod federation_keys_tests;
#[cfg(test)]
mod tests;

pub use argument::{AUTO_PARAM_NAMES, ArgumentDefinition, AutoParams};
pub use auth_email::{
    EmailKind, EmailTemplate, LocalEmailConfig, check_email_header, render_email_template,
    render_link_template,
};
pub use directive::{DirectiveDefinition, DirectiveLocationKind};
pub use fact_table_links::fact_field;
pub use federation_keys::FederationKeyProblem;
pub use localized_indexes::LocalizedIndexAdvice;
pub use mutation::{CascadeSource, InputStyle, MutationDefinition, MutationOperation};
pub use otp_allowed_query::otp_allowed_statement;
pub use query::{CursorType, NativeColumn, PaginationOrder, QueryDefinition};
pub use schema::{
    AppleSocialConfig, AuthClientConfig, CURRENT_FRAISEQL_VERSION, CompiledSchema, DerivedState,
    DiscordSocialConfig, FacebookSocialConfig, GitHubSocialConfig, GoogleSocialConfig,
    LocalAuthConfig, PkceClientConfig, ProducerVersion, SessionConfig, SocialAuthConfig,
    SubscribableEntity,
};
pub use schema_serde::{canonicalize_json, content_hash_of};
pub use validation::is_safe_sql_identifier;
