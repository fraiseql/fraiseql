//! Field-level `requires_scope`: refusal of the shape the runtime cannot honour.
//!
//! # What was broken (ruling Y 7)
//!
//! A scope is granted only by a role: [`SecurityContext::can_access_scope`] asks the role
//! definitions of the `security` section, never the principal's own claims. A schema that
//! declares `requires_scope` without that section therefore declares a gate no principal
//! can pass — and the runtime used to skip field RBAC entirely when the section was absent,
//! so every scoped field was served to every caller, authenticated or not.
//!
//! The section is absent in the ordinary case: `fraiseql compile` writes it only from a
//! `fraiseql.toml`, and without one it logs "using default security configuration" while
//! leaving it out.
//!
//! The runtime now applies the gate whether or not the section is there (and so denies,
//! since no role grants anything). This refuses the shape at load, naming each field, so an
//! author learns it at deploy rather than from a field that is always masked.
//!
//! [`SecurityContext::can_access_scope`]: crate::security::SecurityContext::can_access_scope

use super::CompiledSchema;

impl CompiledSchema {
    /// Fields that require a scope in a schema with no `security` section — no role is
    /// defined, so no principal can hold the scope. One message per field, empty when the
    /// schema is enforceable.
    #[must_use]
    pub fn scope_violations(&self) -> Vec<String> {
        if self.security.is_some() {
            return Vec::new();
        }
        self.types
            .iter()
            .flat_map(|t| {
                t.fields.iter().filter_map(move |f| {
                    f.requires_scope.as_ref().map(|scope| {
                        format!(
                            "field '{}.{}' requires scope '{scope}', but the schema has no \
                             `security` section, so no role can grant it. Declare the roles \
                             that grant it (`[fraiseql.security]` in fraiseql.toml), or drop \
                             `requires_scope`.",
                            t.name, f.name
                        )
                    })
                })
            })
            .collect()
    }
}
