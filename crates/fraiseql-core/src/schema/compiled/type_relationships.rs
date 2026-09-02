//! Relationships: the declarations REST resource embedding follows, and rejection of the
//! ones no execution path can follow.
//!
//! # What was missing (#1266)
//!
//! [`TypeDefinition::relationships`](crate::schema::TypeDefinition::relationships) had no
//! authoring producer. Every non-test assignment in the workspace wrote the empty vector,
//! the authoring IR had no field for it, and `fraiseql.toml` had no block — so a compiled
//! schema always carried `relationships: []`, and the whole embedding surface
//! (`?select=posts(id,title)`, `?select=posts.count`, `?posts.status=`, the `OpenAPI`
//! relationship properties, the generated `relationships.{ts,rs,go,py}` modules) was
//! reachable only from a hand-written `schema.compiled.json`.
//!
//! Giving it a surface makes the checks below possible, and they are the point: four
//! silent wrong answers — #863, #864, #1170, #1230 — lived inside code no author could
//! reach, and #1230 in particular was a join key the projection omitted, which is exactly
//! what a declaration-time check catches.
//!
//! # The shapes this rejects
//!
//! An embed reads a join key off the declaring type's row, then filters the target type's
//! *list query* on the mirror key. Each of those three things can be absent, and every
//! absence today fails the same way: `[]` for a collection, `null` for an object, under a
//! 200 — indistinguishable from "there is genuinely nothing related".
//!
//! * **the target type is not declared** — `find_type` misses, `required_join_keys` returns nothing
//!   and the executor cannot build a predicate;
//! * **a join column names no declared field** — `declared_key` falls back to the column as
//!   written, the projected parent row has no such key, and `extract_join_key` yields `None`
//!   (#1230's shape, one layer earlier);
//! * **the target type has no list query** — `find_list_query_for_type` returns `None` and
//!   `embed_into_single` sets the empty default before touching the database.
//!
//! Two further shapes are refused for the same reason: an empty `foreign_key`/
//! `referenced_key` (both carry `#[serde(default)]`, so absence is an empty string rather
//! than a parse error), and two relationships sharing a name on one type, where the
//! executor's `find` silently takes the first.
//!
//! Checked at load rather than only at compile because `fraiseql compile` refuses to
//! *emit* such a document, which leaves the hand-edited artifact — the case a
//! compile-time check cannot reach, and the reason this runs on the load path every entry
//! point shares (#1262).

use std::collections::HashSet;

use super::CompiledSchema;

impl CompiledSchema {
    /// Relationships declared in a shape no embed can execute.
    ///
    /// Returns one human-readable message per violation, empty when every relationship is
    /// followable. Called from `finish_load` so a schema carrying an unexecutable
    /// relationship never loads.
    ///
    /// The join columns are resolved with the same two functions the REST executor uses —
    /// [`Relationship::parent_join_column`](crate::schema::Relationship::parent_join_column)
    /// and [`TypeDefinition::field_for_column`](crate::schema::TypeDefinition::field_for_column)
    /// — so this cannot accept a schema the executor would fail to follow, nor refuse one
    /// it would have handled.
    #[must_use]
    pub fn relationship_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();

        for type_def in &self.types {
            let mut seen: HashSet<&str> = HashSet::new();
            for rel in &type_def.relationships {
                let owner = type_def.name.as_str();
                let name = rel.name.as_str();

                if !seen.insert(name) {
                    violations.push(format!(
                        "type '{owner}' declares relationship '{name}' more than once; an \
                         embed resolves the first and the rest are unreachable"
                    ));
                    continue;
                }

                if rel.foreign_key.is_empty() || rel.referenced_key.is_empty() {
                    violations.push(format!(
                        "relationship '{owner}.{name}' leaves {} empty; an embed joins on \
                         both columns and cannot compose a predicate without them",
                        if rel.foreign_key.is_empty() {
                            "`foreign_key`"
                        } else {
                            "`referenced_key`"
                        }
                    ));
                    continue;
                }

                let Some(target) = self.find_type(&rel.target_type) else {
                    violations.push(format!(
                        "relationship '{owner}.{name}' targets type '{}', which the schema \
                         does not declare; the embed would resolve to no rows",
                        rel.target_type
                    ));
                    continue;
                };

                let parent_col = rel.parent_join_column();
                if type_def.field_for_column(parent_col).is_none() {
                    violations.push(format!(
                        "relationship '{owner}.{name}' joins on '{parent_col}' of '{owner}', \
                         which declares no such field; the embed reads that key off an \
                         already-projected parent row, so it would find nothing and serve \
                         an empty embed under a 200 (#1230)"
                    ));
                }

                let target_col = rel.target_join_column();
                if target.field_for_column(target_col).is_none() {
                    violations.push(format!(
                        "relationship '{owner}.{name}' filters '{}' on '{target_col}', which \
                         declares no such field; the join predicate is composed against the \
                         published surface and would be refused by the `where` parser",
                        rel.target_type
                    ));
                }

                if !self.queries.iter().any(|q| q.return_type == rel.target_type && q.returns_list)
                {
                    violations.push(format!(
                        "relationship '{owner}.{name}' targets '{}', which no list query \
                         returns; an embed sources its rows from the target's list query, \
                         so this one would serve an empty embed under a 200",
                        rel.target_type
                    ));
                }
            }
        }

        violations
    }
}
