//! Proc macros for the FraiseQL Rust authoring SDK.
//!
//! Do not use this crate directly — import via `fraiseql-rust`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, ItemFn};

/// Mark a struct as a FraiseQL GraphQL object type.
///
/// Introspects fields via syn, registers the type with the thread-local
/// [`SchemaRegistry`](fraiseql_rust::registry::SchemaRegistry) at startup,
/// and derives the `serde::Serialize` implementation required for JSON export.
///
/// # Example
///
/// ```rust,ignore
/// use fraiseql_rust::prelude::*;
///
/// #[fraiseql_type]
/// struct User {
///     id: i32,
///     name: String,
///     #[fraiseql(nullable)]
///     email: Option<String>,
///     #[fraiseql(requires_scope = "read:User.salary")]
///     salary: Option<i64>,
/// }
/// ```
#[proc_macro_attribute]
pub fn fraiseql_type(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    // TODO: introspect fields, build TypeDefinition, call SchemaRegistry::register_type()
    // For now: pass through unchanged + attach a no-op registration call.

    let expanded = quote! {
        #[derive(serde::Serialize)]
        #input

        // Registration will be wired here once SchemaRegistry supports
        // compile-time field introspection via the macro.
    };

    TokenStream::from(expanded)
}

/// Mark a struct as a FraiseQL GraphQL input type.
///
/// Input types are used as mutation arguments. Same field introspection as
/// [`fraiseql_type`] but emitted as `"kind": "INPUT_OBJECT"` in the schema.
///
/// # Example
///
/// ```rust,ignore
/// #[fraiseql_input]
/// struct CreateUserInput {
///     name: String,
///     email: String,
/// }
/// ```
#[proc_macro_attribute]
pub fn fraiseql_input(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    // TODO: identical to fraiseql_type but with kind = INPUT_OBJECT
    let expanded = quote! {
        #[derive(serde::Serialize, serde::Deserialize)]
        #input
    };

    TokenStream::from(expanded)
}

/// Mark an enum as a FraiseQL GraphQL enum type.
///
/// # Example
///
/// ```rust,ignore
/// #[fraiseql_enum]
/// enum UserRole {
///     Admin,
///     Editor,
///     Viewer,
/// }
/// ```
#[proc_macro_attribute]
pub fn fraiseql_enum(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    // TODO: introspect variants, register as EnumTypeDefinition
    let expanded = quote! {
        #[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
        #input
    };

    TokenStream::from(expanded)
}

/// Mark a function as a FraiseQL GraphQL query.
///
/// Attributes:
/// - `sql_source` — the database view or table backing this query (required)
/// - `description` — description for the schema
///
/// # Example
///
/// ```rust,ignore
/// #[fraiseql_query(sql_source = "v_users")]
/// fn users(limit: Option<i32>, offset: Option<i32>) -> Vec<User> {}
///
/// #[fraiseql_query(sql_source = "v_user_by_id", description = "Fetch a single user")]
/// fn user(id: i32) -> Option<User> {}
/// ```
#[proc_macro_attribute]
pub fn fraiseql_query(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    // TODO: extract function signature (arg names/types, return type),
    // parse attr for sql_source/description, register as QueryDefinition
    let expanded = quote! {
        #input
    };

    TokenStream::from(expanded)
}

/// Mark a function as a FraiseQL GraphQL mutation.
///
/// # Example
///
/// ```rust,ignore
/// #[fraiseql_mutation(sql_source = "fn_create_user")]
/// fn create_user(input: CreateUserInput) -> User {}
/// ```
#[proc_macro_attribute]
pub fn fraiseql_mutation(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    // TODO: same as fraiseql_query but kind = MUTATION
    let expanded = quote! { #input };

    TokenStream::from(expanded)
}

/// Mark a function as a FraiseQL GraphQL subscription.
///
/// # Example
///
/// ```rust,ignore
/// #[fraiseql_subscription(topic = "user_events")]
/// fn on_user_updated(user_id: i32) -> User {}
/// ```
#[proc_macro_attribute]
pub fn fraiseql_subscription(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    // TODO: register as SubscriptionDefinition with topic
    let expanded = quote! { #input };

    TokenStream::from(expanded)
}
