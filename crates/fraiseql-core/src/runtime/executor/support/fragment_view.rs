//! A document's fragments judged against the types a caller may see.
//!
//! A fragment's type condition resolves against the schema the caller may see, the same as
//! a variable's type (§ 5.8.2): a type outside it answers as a type the schema does not
//! define. The executor's own dispatch does this when it classifies a document, per view.
//! This is the same check for a transport that plans from the document before the
//! executor runs it.

use crate::{
    error::Result,
    graphql::{ParsedQuery, fragment_conditions},
    runtime::Executor,
    security::SecurityContext,
};

impl Executor {
    /// Refuse a fragment of `parsed` whose type condition names no type `caller` may see,
    /// or one that cannot apply where it stands
    /// ([`fragment_conditions::check`]).
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`](crate::error::FraiseQLError::Validation) at the
    /// fragment's path, naming it and its type.
    pub fn check_fragment_conditions(
        &self,
        parsed: &ParsedQuery,
        caller: Option<&SecurityContext>,
    ) -> Result<()> {
        let visible = self.ctx.introspection.caller_schema(&self.ctx.schema, caller);
        fragment_conditions::check(
            &parsed.selections,
            &parsed.fragments,
            &parsed.operation_type,
            &self.ctx.schema,
            visible.as_deref().unwrap_or(&self.ctx.schema),
        )
    }
}
