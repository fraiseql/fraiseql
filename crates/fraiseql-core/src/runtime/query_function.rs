//! Resolution of a function-backed root query field (#1329).
//!
//! The engine half of the `request:query` trigger. A query that declares
//! [`function = "<name>"`](crate::schema::QueryDefinition::function) is answered by a
//! sandboxed function instead of by reading a relation, and this is the seam through
//! which the engine reaches one — parallel to
//! [`BeforeMutationGate`](crate::security::BeforeMutationGate), and for the same
//! reason: `fraiseql-core` knows nothing about function runtimes, so it holds a
//! trait object the application supplies. In `fraiseql-server` that object dispatches
//! into the compiled schema's `functions` section.
//!
//! # What the engine keeps
//!
//! Everything except the value. The resolver is asked for the field's *data*; the
//! engine still:
//!
//! - enforces `requires_role` and `requires_actor`, before asking;
//! - applies field-level RBAC to what comes back, so a function-backed field is not a hole in the
//!   scope gates every other field passes through;
//! - projects the selection set, recases, and stamps `__typename` with the same projector the SQL
//!   path uses;
//! - consults and populates the response cache on the same terms as any other read (which today
//!   means "as little as any other read": `fraiseql-server` installs no `ResponseCache` — #1344).
//!
//! That list is the argument for option A in #1329. A field that resolved outside
//! the engine would have to re-implement every one of them, and would be wrong about
//! one of them within a release.
//!
//! # Root fields only
//!
//! There is no nested-field equivalent and there will not be one. A nested resolver
//! runs once per row, so a function there is an N+1 measured in V8 isolates —
//! ~5–8 ms each, against a documented cold read of ~5–15 ms for the whole query.
//! Root-only makes "one invocation per query" a property of the shape, and the
//! compiler refuses the nested spelling rather than documenting against it.

use std::{future::Future, pin::Pin, sync::Arc};

use crate::{
    error::Result,
    security::{GuestQueryBridge, SecurityContext},
};

/// The field a [`QueryFunctionResolver`] is being asked to answer.
#[non_exhaustive]
pub struct QueryFunctionRequest<'a> {
    /// The declared function name, from the compiled query's `function` key.
    ///
    /// Never the field name: one function may back several fields, and the compiler
    /// checks the pairing in both directions, so this is the only name the dispatcher
    /// needs to look anything up by.
    pub function:  &'a str,
    /// The root query field being answered.
    ///
    /// Passed so the guest can tell which field it was invoked for when one function
    /// backs several, and so a failure names the field rather than the module.
    pub field:     &'a str,
    /// The field's resolved arguments: request variables merged with the root field's
    /// inline literals, nested `$var` references already substituted. An empty object
    /// when the field takes none.
    pub arguments: &'a serde_json::Value,
    /// The requesting principal, or `None` for an anonymous request.
    pub principal: Option<&'a SecurityContext>,
    /// The caller-scoped, read-only GraphQL bridge (#1328).
    ///
    /// The same bridge a `before:mutation` hook reads through, built from this
    /// request's own executor context and principal — deliberately not a second one.
    /// A function-backed field that reads sees exactly what its caller could have
    /// read, and cannot write.
    pub reader:    Arc<dyn GuestQueryBridge>,
}

/// Answers a function-backed root query field.
///
/// Registered on [`RuntimeConfig`](crate::runtime::RuntimeConfig) via
/// [`with_query_function_resolver`](crate::runtime::RuntimeConfig::with_query_function_resolver),
/// parallel to
/// [`with_before_mutation_gate`](crate::runtime::RuntimeConfig::with_before_mutation_gate).
///
/// # The return value is the field's data, not a response
///
/// For a single-item field: the entity document, or `null`. For a list field: an
/// array of them. The engine projects the selection set over it, so the guest
/// returns whatever the type declares and never a GraphQL envelope — which also
/// means a guest cannot invent a field the schema does not declare, or return one
/// the caller's scopes deny.
pub trait QueryFunctionResolver: Send + Sync {
    /// Invoke the function that backs this field.
    ///
    /// # Errors
    ///
    /// Any error refuses the field. A resolver is expected to map its own failures —
    /// a missing module, a guest exception, a timeout — to an error that names the
    /// field, because by the time the engine sees it the only other thing it could
    /// say is "the query failed".
    fn resolve<'a>(
        &'a self,
        request: QueryFunctionRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + 'a>>;
}
