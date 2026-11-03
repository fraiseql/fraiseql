//! The rows of a fact table's aggregate root, projected onto its selection set (#1800).
//!
//! The root publishes `[<Stem>AggregateRow!]!`, every field nullable, while the runtime
//! computes a row from the root's arguments (`groupBy`, `aggregates`). Clients read the
//! published type, so each row is rebuilt from the selection set: a selected field answers
//! under its response key, with the computed value or `null` when the arguments did not
//! compute it, `__typename` names the row type, and a computed key nothing selected is
//! dropped.
//!
//! An `Int` is a signed 32-bit integer (§ 3.5.1), and a count or an `int8` group key is
//! wider: a value outside that range is a field error, its field `null`, never a number the
//! type says cannot occur.

use serde_json::{Map, Value};

use super::{OutputTypes, Step, TypeRef, Walk, response_path};
use crate::graphql::FieldSelection;

impl OutputTypes {
    /// Whether the schema publishes an object type named `type_name`.
    #[must_use]
    pub fn publishes_object(&self, type_name: &str) -> bool {
        self.fields.contains_key(type_name)
    }

    /// Why `selections` cannot be a selection set on `type_name`, an object type whose fields
    /// are all leaves (§ 5.3.1, § 5.3.3): the fields it does not publish, then the fields given
    /// a selection set. `None`
    /// when every selected field is a published field selected as its type requires.
    ///
    /// `selections` has its fragment spreads expanded; `variables` evaluates `@skip` and
    /// `@include`.
    #[must_use]
    pub fn row_selection_violation(
        &self,
        type_name: &str,
        selections: &[FieldSelection],
        variables: Option<&Value>,
    ) -> Option<String> {
        let (type_name, fields) = self.type_fields(type_name)?;
        let walk = Walk::new(self, variables);
        let mut unpublished = Vec::new();
        let mut leaves = Vec::new();
        walk.for_each_field(selections, type_name, &mut |sel| {
            if !fields.contains_key(&sel.name) {
                unpublished.push(format!("`{}`", sel.name));
            } else if !sel.nested_fields.is_empty() {
                leaves.push(format!("`{type_name}.{}`", sel.name));
            }
        });
        if !unpublished.is_empty() {
            return Some(format!("`{type_name}` has no field {}", unpublished.join(", ")));
        }
        if !leaves.is_empty() {
            return Some(format!("{} is a leaf and takes no selection set", leaves.join(", ")));
        }
        None
    }

    /// Project `rows`, the computed rows of an aggregate root answering under `root_key`,
    /// onto `selections`, a selection set on the object type `type_name` whose fields are
    /// scalars: each row holds the selected fields, in document order, under their response
    /// keys. Returns the rows and the field errors, their paths rooted at `root_key`.
    ///
    /// `selections` has its fragment spreads expanded; `variables` evaluates `@skip` and
    /// `@include`.
    #[must_use]
    pub fn project_rows(
        &self,
        rows: Vec<Value>,
        type_name: &str,
        selections: &[FieldSelection],
        variables: Option<&Value>,
        root_key: &str,
    ) -> (Value, Vec<Value>) {
        let walk = Walk::new(self, variables);
        let fields = self.type_fields(type_name).map(|(_, fields)| fields);
        let mut errors = Vec::new();
        let projected = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let Value::Object(computed) = row else {
                    return row;
                };
                let mut out = Map::new();
                for sel in selections {
                    if !walk.included(sel) {
                        continue;
                    }
                    walk.select(sel, type_name, &mut |field| {
                        let key = field.response_key();
                        let value = if field.name == "__typename" {
                            Value::String(type_name.to_string())
                        } else {
                            // Read, not taken: two aliases may select the same field.
                            let value = computed.get(&field.name).cloned().unwrap_or(Value::Null);
                            let is_int = matches!(
                                fields.and_then(|f| f.get(&field.name)),
                                Some(TypeRef::Named(name)) if name == "Int"
                            );
                            if is_int && !fits_an_int(&value) {
                                let path =
                                    [Step::Key(root_key), Step::Index(index), Step::Key(key)];
                                errors.push(int_out_of_range(
                                    type_name,
                                    &field.name,
                                    &value,
                                    &path,
                                ));
                                Value::Null
                            } else {
                                value
                            }
                        };
                        out.insert(key.to_string(), value);
                    });
                }
                Value::Object(out)
            })
            .collect();
        (Value::Array(projected), errors)
    }
}

impl Walk<'_> {
    /// Visit `sel` as a selection on an object of type `type_name`: itself when it is a
    /// field, `__typename` included, or the fields of an inline fragment whose condition the
    /// type satisfies. A field is visited once per occurrence; a later one with the same
    /// response key overwrites the earlier, which § 5.3.2 makes the same field.
    fn select<'s>(
        &self,
        sel: &'s FieldSelection,
        type_name: &str,
        visit: &mut impl FnMut(&'s FieldSelection),
    ) {
        if let Some(condition) = sel.name.strip_prefix("...on ") {
            if self.satisfies(type_name, condition.trim()) {
                for nested in &sel.nested_fields {
                    if self.included(nested) {
                        self.select(nested, type_name, visit);
                    }
                }
            }
            return;
        }
        if sel.name.starts_with("...") || (sel.name.starts_with("__") && sel.name != "__typename") {
            return;
        }
        visit(sel);
    }
}

/// Whether `value` is `null`, or not a number, or a number an `Int` holds.
fn fits_an_int(value: &Value) -> bool {
    let Value::Number(number) = value else {
        return true;
    };
    number.as_i64().is_some_and(|n| i32::try_from(n).is_ok())
}

/// The field error for a computed value outside an `Int`'s range.
fn int_out_of_range(type_name: &str, field: &str, value: &Value, path: &[Step<'_>]) -> Value {
    serde_json::json!({
        "message": format!(
            "`{type_name}.{field}` is an `Int`, a signed 32-bit integer, and the computed \
             value {value} is outside its range"
        ),
        "path": response_path(path),
    })
}
