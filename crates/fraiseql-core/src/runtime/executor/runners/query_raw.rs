//! A root read answered as the bytes the database serialized.
//!
//! The projection already shapes a row the way the response wants it — its keys the
//! response keys, in selection order, each nested object built from its selected
//! sub-fields. Where nothing after the read would change a byte of it, the rows are
//! spliced into the `data` envelope as the database wrote them, and no row becomes a
//! [`serde_json::Value`].
//!
//! Anything Rust would still reshape keeps the read on the `Value` path: `__typename`,
//! a localized, encrypted or computed field, an enum, a list, a float or a JSON scalar,
//! an abstract type, a mask, a gated field, a nested level read by a composed statement.

use std::{collections::HashSet, sync::Arc};

use super::{query::QueryRunner, root_read::RootRead};
use crate::{
    backend::{RawRows, types::JsonbValue},
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::QueryMatch,
    schema::{CompiledSchema, FieldType},
};

/// What a raw read returned.
pub(super) enum RawRead {
    /// The response body, complete.
    Body(Vec<u8>),
    /// Rows that serialize differently once parsed, or that completion would change:
    /// parsed, for the `Value` path to shape from here, with no second read.
    Rows(Arc<Vec<JsonbValue>>),
}

impl QueryRunner {
    /// Read `read` as the database's own text, when its rows need no shaping in Rust.
    ///
    /// `Ok(None)` when the read is not one, or the adapter does not answer raw: nothing
    /// was read, and the caller reads it as `Value`s.
    ///
    /// # Errors
    ///
    /// The adapter's errors, and the response ceiling's refusal.
    pub(super) async fn read_raw(
        &self,
        read: &RootRead,
        query_match: &QueryMatch,
        session_pairs: &[(&str, &str)],
    ) -> Result<Option<RawRead>> {
        let Some(shape) = RawShape::of(read, query_match, &self.ctx.schema) else {
            return Ok(None);
        };
        let query_def = &query_match.query_def;
        let Some(rows) = self
            .ctx
            .adapter
            .execute_with_projection_raw_with_session(
                &read.projection_request(),
                session_pairs,
                query_def.read_routing,
            )
            .await?
        else {
            return Ok(None);
        };

        // § 6.4.4 completion leaves the response as it is unless a non-null position holds
        // `null`: a selected non-null field, or a non-null single root with no row.
        let completes_unchanged = !(shape.non_null && rows.has_null())
            && (query_def.returns_list || query_def.nullable || !rows.rows().is_empty());
        if !rows.exact() || !completes_unchanged {
            let parsed = rows.parse().map_err(|e| FraiseQLError::Internal {
                message: format!("a row the database serialized is not valid JSON: {e}"),
                source:  None,
            })?;
            return Ok(Some(RawRead::Rows(Arc::new(
                parsed.into_iter().map(JsonbValue::new).collect(),
            ))));
        }

        if let Some(budget) = read.budget() {
            budget.charge_raw_rows(&rows)?;
        }
        Ok(Some(RawRead::Body(envelope(
            &rows,
            query_match.response_key(),
            query_def.returns_list,
        ))))
    }
}

/// `{"data":{"<key>":<rows>}}`: the rows as a list, or the first row (`null` without one).
fn envelope(rows: &RawRows, response_key: &str, is_list: bool) -> Vec<u8> {
    let mut body = Vec::with_capacity(rows.bytes().len() + response_key.len() + 16);
    body.extend_from_slice(b"{\"data\":{");
    // A GraphQL name needs no escaping, but the key is written as `serde_json` writes one.
    serde_json::to_writer(&mut body, response_key).unwrap_or_default();
    body.push(b':');
    if is_list {
        body.push(b'[');
        body.extend_from_slice(rows.bytes());
        body.push(b']');
    } else {
        body.extend_from_slice(rows.row(0).unwrap_or(b"null"));
    }
    body.extend_from_slice(b"}}");
    body
}

/// A read whose rows need no shaping in Rust, and whether its selection holds a non-null
/// field.
struct RawShape {
    non_null: bool,
}

impl RawShape {
    fn of(read: &RootRead, query_match: &QueryMatch, schema: &CompiledSchema) -> Option<Self> {
        if !Self::flat(read) {
            return None;
        }
        let root = query_match.selections.first()?;
        let mut shape = Self { non_null: false };
        shape
            .admits(&root.nested_fields, schema, &query_match.query_def.return_type)
            .then_some(shape)
    }

    /// Whether the read is one projected statement over a relation, every row served as
    /// it is: no mask to null, no gated field to adjudicate, no row to project as its own
    /// type, no nested level to merge, no `nearest` search to settle.
    fn flat(read: &RootRead) -> bool {
        read.projection.is_some()
            && read.nested.is_empty()
            && read.source_args.is_none()
            && !read.gated_present
            && !read.abstract_read
            && read.nearest_k.is_none()
            && read.nearest_count.is_none()
            && read.selection.masks_nothing()
    }

    /// Whether every field of `selections`, at every depth, is served as the projection
    /// reads it.
    ///
    /// A nested object is admitted only through its own selected sub-fields, each admitted
    /// in turn: the projection builds such an object from them, and a value read whole
    /// from below a field is never served as it was stored.
    fn admits(
        &mut self,
        selections: &[FieldSelection],
        schema: &CompiledSchema,
        type_name: &str,
    ) -> bool {
        let Some(type_def) = schema.find_type(type_name) else {
            return false;
        };
        let mut keys = HashSet::with_capacity(selections.len());
        selections.iter().all(|sel| {
            // Neither `__typename` nor a localized field's translations sibling is a declared
            // field, so the lookup below refuses both.
            if !sel.arguments.is_empty() || !keys.insert(sel.response_key()) {
                return false;
            }
            let Some(field) = type_def.fields.iter().find(|f| f.name == sel.name.as_str()) else {
                return false;
            };
            if field.localized
                || field.authorize
                || field.encryption.is_some()
                || field.vector_distance.is_some()
                || field.alias.is_some()
                || field.hierarchy.is_some()
            {
                return false;
            }
            self.non_null |= !field.nullable;
            match &field.field_type {
                FieldType::String
                | FieldType::Id
                | FieldType::Uuid
                | FieldType::Int
                | FieldType::Boolean => sel.nested_fields.is_empty(),
                FieldType::Object(child) => {
                    !sel.nested_fields.is_empty() && self.admits(&sel.nested_fields, schema, child)
                },
                _ => false,
            }
        })
    }
}

#[cfg(test)]
#[path = "query_raw_tests.rs"]
mod tests;
