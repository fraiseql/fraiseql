//! A read whose return type is an interface or a union (#1612): a read of each of the
//! object types its rows may be.
//!
//! Each row is of one object type, and answers as it: `__typename` names that type, never
//! the interface (§ 3.7: an abstract type cannot be a value's runtime type), and the
//! inline fragments it satisfies are applied, as completion applies them to a response.
//! The type is the one the row's own `__typename` names, when the view stores one; else
//! the one possible type whose view the query reads; a row neither says is refused.
//!
//! So that a field reached through a fragment is gated as the same field selected on its
//! type is, every possible type's selections are classified (a `Reject` scope refuses, a
//! `Mask` scope is nulled on the rows of that type), and a policy-gated field (#423) that a
//! possible type would serve refuses the read, as a nested gated field does. The rows are
//! read whole and projected here, each as its type.
//!
//! REST and gRPC derive no route for a query whose return type is not an object type, so
//! the GraphQL read runners are the paths that reach this.

use crate::{
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    schema::{CompiledSchema, QueryDefinition},
};

/// The object types a read typed as an interface or a union may return.
pub(super) struct AbstractRead<'s> {
    /// The query.
    query:     &'s QueryDefinition,
    /// The object types its rows may be: the interface's implementors, the union's members.
    possible:  Vec<&'s str>,
    /// The one possible type whose own view the query reads, if exactly one does.
    view_type: Option<&'s str>,
}

impl<'s> AbstractRead<'s> {
    /// `Some` when `query` returns an interface or a union.
    pub(super) fn of(schema: &'s CompiledSchema, query: &'s QueryDefinition) -> Option<Self> {
        let name = query.return_type.as_str();
        let possible: Vec<&str> = if schema.find_interface(name).is_some() {
            schema
                .types
                .iter()
                .filter(|t| t.implements.iter().any(|i| i == name))
                .map(|t| t.name.as_str())
                .collect()
        } else {
            schema.find_union(name)?.member_types.iter().map(String::as_str).collect()
        };
        let mut reading_the_view = possible.iter().copied().filter(|t| {
            schema.find_type(t).is_some_and(|td| {
                query.sql_source.as_deref().is_some_and(|view| td.sql_source.as_str() == view)
            })
        });
        let view_type = match (reading_the_view.next(), reading_the_view.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        };
        Some(Self {
            query,
            possible,
            view_type,
        })
    }

    /// Refuse a policy-gated field (#423) that a possible type would serve: the per-row
    /// authorizer decides on rows of one declared type, and a read of several cannot be
    /// put to it field by field. Fail closed, as a nested gated field is.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` naming the type whose gated field is selected.
    pub(super) fn refuse_gated_fields(
        &self,
        schema: &CompiledSchema,
        root_fields: &[FieldSelection],
    ) -> Result<()> {
        let Some(gated) = self.possible.iter().find(|t| {
            crate::security::field_authorizer::selection_set_selects_gated_field(
                schema,
                t,
                root_fields,
            )
        }) else {
            return Ok(());
        };
        Err(FraiseQLError::Authorization {
            message:  format!(
                "'{}' returns the abstract type '{}', and the selection reaches a policy-gated \
                 field of '{gated}', which field-level authorization does not support through \
                 an abstract type; query '{gated}' directly",
                self.query.name, self.query.return_type
            ),
            action:   Some("read".to_string()),
            resource: Some((*gated).to_string()),
        })
    }

    /// Every possible type's selections, classified as a read of that type.
    ///
    /// # Errors
    ///
    /// As [`SelectionAccess::classify_roots`](super::query_nested::SelectionAccess::classify_roots).
    pub(super) fn classify(
        &self,
        schema: &CompiledSchema,
        root_fields: &[FieldSelection],
        security_context: Option<&crate::security::SecurityContext>,
        authz: Option<super::query_nested::LevelAuthz<'_>>,
    ) -> Result<super::query_nested::SelectionAccess> {
        let roots: Vec<(&str, &[FieldSelection])> =
            self.possible.iter().map(|t| (*t, root_fields)).collect();
        super::query_nested::SelectionAccess::classify_roots(
            schema,
            &roots,
            security_context,
            authz,
        )
    }

    /// The object type `row` is.
    fn type_of(&self, row: &serde_json::Value) -> Result<&'s str> {
        match row.get("__typename").and_then(serde_json::Value::as_str) {
            Some(named) => self.possible.iter().copied().find(|t| *t == named).ok_or_else(|| {
                self.refusal(&format!(
                    "a row's `__typename` is '{named}', which is not one of its types ({})",
                    self.possible.join(", ")
                ))
            }),
            None => self.view_type.ok_or_else(|| {
                self.refusal(&format!(
                    "its view holds rows of several of its types ({}) and a row names none; \
                     store `__typename` in each row's data, or read a view of one type",
                    self.possible.join(", ")
                ))
            }),
        }
    }

    fn refusal(&self, why: &str) -> FraiseQLError {
        FraiseQLError::Validation {
            message: format!(
                "Query '{}' returns the abstract type '{}' and {why}",
                self.query.name, self.query.return_type
            ),
            path:    None,
        }
    }

    /// The rows, each projected as the object type it is, with the fields `access` masks
    /// on that type nulled.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Validation` for a row whose type cannot be told.
    pub(super) fn project(
        &self,
        documents: &[crate::backend::JsonbValue],
        root_fields: &[FieldSelection],
        schema: &CompiledSchema,
        access: &super::query_nested::SelectionAccess,
    ) -> Result<serde_json::Value> {
        let project = |document: &crate::backend::JsonbValue| -> Result<serde_json::Value> {
            let type_name = self.type_of(&document.data)?;
            let mut row =
                crate::runtime::project_entity(&document.data, type_name, root_fields, schema);
            access.null_masked(&mut row, type_name, root_fields, schema);
            Ok(row)
        };
        if self.query.returns_list {
            documents
                .iter()
                .map(project)
                .collect::<Result<Vec<_>>>()
                .map(serde_json::Value::Array)
        } else {
            documents.first().map_or(Ok(serde_json::Value::Null), project)
        }
    }
}
