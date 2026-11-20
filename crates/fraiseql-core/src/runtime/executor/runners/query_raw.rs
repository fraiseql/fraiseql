//! A root read answered as the bytes the database serialized.
//!
//! The projection already shapes a row the way the response wants it — its keys the
//! response keys, in selection order, each nested object and each element of a list of
//! objects built from its selected sub-fields. Where nothing after the read would change a
//! byte of it, the rows are spliced into the `data` envelope as the database wrote them,
//! and no row becomes a [`serde_json::Value`]. Only their numbers are written again, as
//! `serde_json` writes them.
//!
//! A value its scalar does not hold as served, under a key an `Int`, `Float`, `String` or
//! `Boolean` field is selected under, sends the rows to the `Value` path too: a number no
//! `Int` holds as written, a string under an `Int`, a number under a `String`, an object
//! under any of them. Completion coerces it or makes it a field error (§ 3.5).
//!
//! Anything Rust would still reshape keeps the read on the `Value` path: a localized,
//! encrypted or computed field, an enum, a list of anything but objects, a JSON scalar, an
//! abstract type and any `__typename` below one, a mask the projection did not write, a
//! gated field, a nested level read by a composed statement. A `__typename` of an object
//! type is the literal the projection writes, and so is the `null` of a field the caller's
//! scopes mask.

use std::{collections::HashSet, sync::Arc};

use super::{query::QueryRunner, root_read::RootRead};
use crate::{
    backend::{ProjectionRequest, RawRows, ScalarKind, types::JsonbValue},
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
        let selections = query_match
            .selections
            .first()
            .map_or(&[][..], |root| root.nested_fields.as_slice());
        let schema = &self.ctx.schema;
        let return_type = &query_def.return_type;
        // A read told its scalar keys answers whether a value of another kind sits under one
        // as the rows are copied ([`RawShape::told`]).
        let told_keys = if shape.told() {
            scalar_keys(selections, schema, return_type)
        } else {
            Vec::new()
        };
        let Some(rows) = self
            .ctx
            .adapter
            .execute_with_projection_raw_with_session(
                &ProjectionRequest {
                    scalar_keys: shape.told().then_some(&told_keys[..]),
                    ..read.projection_request()
                },
                session_pairs,
                query_def.read_routing,
            )
            .await?
        else {
            return Ok(None);
        };

        // § 6.4.4 completion leaves the response as it is unless a non-null position holds
        // `null` (a selected non-null field, or a non-null single root with no row), or a
        // built-in scalar position holds a value of another kind (§ 3.5).
        let completes_unchanged =
            !shape.may_hold_null_at_non_null(&rows, selections, schema, return_type)
                && !shape.may_hold_wrong_kind(&rows, &told_keys, selections, schema, return_type)
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

/// A read whose rows need no shaping in Rust, and what its selection holds, at any depth: a
/// non-null field, an object field, an `Int`, `Float`, `String` or `Boolean` field, and one
/// of those other than a `String`.
struct RawShape {
    non_null:    bool,
    objects:     bool,
    scalars:     bool,
    non_strings: bool,
}

impl RawShape {
    /// Whether `rows`, read through `selections` of a `type_name`, may hold `null` at a
    /// non-null field: under a key a non-null field is selected under, at any depth, or
    /// where no key names it. Under any other key a `null` is a nullable field's, a mask's
    /// among them, and needs no completion.
    ///
    /// The selection is walked only for a key a `null` sits under.
    fn may_hold_null_at_non_null(
        &self,
        rows: &RawRows,
        selections: &[FieldSelection],
        schema: &CompiledSchema,
        type_name: &str,
    ) -> bool {
        self.non_null
            && rows.has_null_where(|key| {
                key.is_none_or(|key| selects_non_null_under(selections, schema, type_name, key))
            })
    }

    /// Whether the adapter is told the read's scalar keys ([`scalar_keys`]), so whether a
    /// value of another kind sits under one is answered as the rows are copied. A read of
    /// no object field selecting no scalar but `String`s is told none: a value of another
    /// kind below a row is then a member of the row, under some key, and the rows' tallies
    /// answer whether one is there at all before any key is looked at.
    const fn told(&self) -> bool {
        self.scalars && (self.objects || self.non_strings)
    }

    /// Whether `rows`, read through `selections` of a `type_name`, may hold a value of
    /// another kind under the key of a selected `Int`, `Float`, `String` or `Boolean`:
    /// asked of `told_keys` when the rows were [`told`](Self::told) them; otherwise of the
    /// keys, walked for only when the rows hold a value no `String` holds.
    fn may_hold_wrong_kind(
        &self,
        rows: &RawRows,
        told_keys: &[(&[u8], ScalarKind)],
        selections: &[FieldSelection],
        schema: &CompiledSchema,
        type_name: &str,
    ) -> bool {
        if !self.scalars {
            return false;
        }
        if self.told() {
            return rows.has_wrong_kind_under(told_keys);
        }
        rows.may_hold_refused_by(&[ScalarKind::String])
            && rows.has_wrong_kind_under(&scalar_keys(selections, schema, type_name))
    }

    /// A shape before any selection is admitted.
    const fn new() -> Self {
        Self {
            non_null:    false,
            objects:     false,
            scalars:     false,
            non_strings: false,
        }
    }

    fn of(read: &RootRead, query_match: &QueryMatch, schema: &CompiledSchema) -> Option<Self> {
        if !Self::flat(read) {
            return None;
        }
        let root = query_match.selections.first()?;
        let mut shape = Self::new();
        shape
            .admits(&root.nested_fields, schema, &query_match.query_def.return_type)
            .then_some(shape)
    }

    /// Whether the read is one projected statement over a relation, every row served as
    /// it is: no mask the projection did not write, no gated field to adjudicate, no row to
    /// project as its own type, no nested level to merge, no `nearest` search to settle.
    ///
    /// A mask the projection writes is at a position it builds from sub-fields, which is
    /// every position [`admits`](Self::admits) admits. A mask of any other kind needs the
    /// row: the field authorizer's is a gated field, and keeps the read off this path.
    fn flat(read: &RootRead) -> bool {
        read.projection.is_some()
            && read.nested.is_empty()
            && read.source_args.is_none()
            && !read.gated_present
            && !read.abstract_read
            && read.nearest_k.is_none()
            && read.nearest_count.is_none()
            && (read.built_in_sql || read.selection.masks_nothing())
    }

    /// Whether every field of `selections`, at every depth, is served as the projection
    /// reads it.
    ///
    /// A nested object, or a list of them, is admitted only through its own selected
    /// sub-fields, each admitted in turn: the projection builds such an object from them,
    /// and a value read whole from below a field is never served as it was stored.
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
            if !sel.arguments.is_empty() || !keys.insert(sel.response_key()) {
                return false;
            }
            // `type_name` is an object type, so its `__typename` is the literal the
            // projection writes. A localized field's translations sibling is not a
            // declared field, so the lookup below refuses it.
            if sel.name == "__typename" {
                return sel.nested_fields.is_empty();
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
            if let Some(kind) = scalar_kind(&field.field_type) {
                self.scalars = true;
                self.non_strings |= kind != ScalarKind::String;
            }
            match &field.field_type {
                FieldType::String
                | FieldType::Id
                | FieldType::Uuid
                | FieldType::Int
                | FieldType::Float
                | FieldType::Decimal
                | FieldType::Boolean => sel.nested_fields.is_empty(),
                ty => object_type(ty).is_some_and(|child| {
                    self.objects = true;
                    !sel.nested_fields.is_empty() && self.admits(&sel.nested_fields, schema, child)
                }),
            }
        })
    }
}

/// Whether `selections` of a `type_name`, at any depth, select a non-null field under the
/// response key `key`.
fn selects_non_null_under(
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    type_name: &str,
    key: &[u8],
) -> bool {
    let Some(type_def) = schema.find_type(type_name) else {
        return true;
    };
    selections.iter().any(|sel| {
        let Some(field) = type_def.fields.iter().find(|f| f.name == sel.name.as_str()) else {
            return false;
        };
        (!field.nullable && sel.response_key().as_bytes() == key)
            || object_type(&field.field_type)
                .is_some_and(|child| selects_non_null_under(&sel.nested_fields, schema, child, key))
    })
}

/// The kind a built-in scalar field's value holds as served, for those § 3.5 adjudicates.
const fn scalar_kind(field_type: &FieldType) -> Option<ScalarKind> {
    match field_type {
        FieldType::Int => Some(ScalarKind::Int),
        FieldType::Float => Some(ScalarKind::Float),
        // An `ID` holds a string as served (§ 3.5.5): an integer is rewritten by completion.
        FieldType::String | FieldType::Id => Some(ScalarKind::String),
        FieldType::Boolean => Some(ScalarKind::Boolean),
        _ => None,
    }
}

/// The response key and kind of every `Int`, `Float`, `String` and `Boolean` field
/// `selections` of a `type_name` select, at any depth: the keys under which a value of
/// another kind may sit. Under any other key a value is served as read.
fn scalar_keys<'s>(
    selections: &'s [FieldSelection],
    schema: &CompiledSchema,
    type_name: &str,
) -> Vec<(&'s [u8], ScalarKind)> {
    let mut keys = Vec::new();
    push_scalar_keys(selections, schema, type_name, &mut keys);
    keys
}

/// Push to `keys` the response key and kind of every `Int`, `Float`, `String` and `Boolean`
/// field `selections` of a `type_name` select, at any depth. A read reaches here only
/// through [`RawShape::admits`], which refuses a type the schema does not declare, so every
/// level is found.
fn push_scalar_keys<'s>(
    selections: &'s [FieldSelection],
    schema: &CompiledSchema,
    type_name: &str,
    keys: &mut Vec<(&'s [u8], ScalarKind)>,
) {
    let Some(type_def) = schema.find_type(type_name) else {
        return;
    };
    for sel in selections {
        let Some(field) = type_def.fields.iter().find(|f| f.name == sel.name.as_str()) else {
            continue;
        };
        if let Some(kind) = scalar_kind(&field.field_type) {
            keys.push((sel.response_key().as_bytes(), kind));
        } else if let Some(child) = object_type(&field.field_type) {
            push_scalar_keys(&sel.nested_fields, schema, child, keys);
        }
    }
}

/// The object type of a single object, or of a list's elements.
fn object_type(field_type: &FieldType) -> Option<&str> {
    match field_type {
        FieldType::Object(child) => Some(child),
        list => list.inner_type().filter(|item| item.is_object()).and_then(FieldType::type_name),
    }
}

#[cfg(test)]
#[path = "query_raw_tests.rs"]
mod tests;
