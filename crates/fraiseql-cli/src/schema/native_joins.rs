//! The native columns a relationship joins on, proven by the database (#1587).
//!
//! A composed read correlates an embedded level to its parent by stored document keys unless
//! the compiled schema says which SQL columns join them. `fraiseql compile --database` reads
//! that from `pg_tviews`' registry: a relationship whose child and parent types read TVIEWs,
//! and whose declared columns are a reference the registry publishes (a child column holding
//! the parent's identity, proven by a `FOREIGN KEY` and indexed). Nothing is guessed: a type
//! reading a plain view, or columns the registry does not link, keeps its stored keys.

use fraiseql_core::schema::{CompiledSchema, NativeJoin};

/// One reference the registry publishes, between two relations as the schema names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TviewReference {
    /// The `sql_source` of the child, whose column references the parent.
    pub child:          String,
    /// The `sql_source` of the parent.
    pub parent:         String,
    /// The child's reference column (`fk_post`).
    pub foreign_key:    String,
    /// The parent's identity column (`pk_post`).
    pub referenced_key: String,
}

/// Record on each relationship of `schema` the native columns `references` prove it joins on.
///
/// A relationship names SQL columns (`foreign_key` on the child, `referenced_key` on the
/// parent); it gets a proof only when a reference between the two types' views holds exactly
/// those two columns. Nothing else is taken, not even the only reference between the views:
/// it may be another relationship's. Returns how many relationships were given one.
pub fn apply_native_joins(schema: &mut CompiledSchema, references: &[TviewReference]) -> usize {
    let sources: std::collections::HashMap<String, String> = schema
        .types
        .iter()
        .map(|t| (t.name.to_string(), t.sql_source.to_string()))
        .collect();
    let mut given = 0;
    for declaring in &mut schema.types {
        for rel in &mut declaring.relationships {
            // The side holding the foreign key is the child: the target of a list, the
            // declaring type of a to-one embed.
            let (child, parent) = if rel.cardinality.is_to_one() {
                (declaring.name.as_str(), rel.target_type.as_str())
            } else {
                (rel.target_type.as_str(), declaring.name.as_str())
            };
            let (Some(child), Some(parent)) = (sources.get(child), sources.get(parent)) else {
                continue;
            };
            rel.native_join = references
                .iter()
                .find(|r| {
                    &r.child == child
                        && &r.parent == parent
                        && r.foreign_key == rel.foreign_key
                        && r.referenced_key == rel.referenced_key
                })
                .map(|r| NativeJoin {
                    foreign_key:    r.foreign_key.clone(),
                    referenced_key: r.referenced_key.clone(),
                    child_view:     child.clone(),
                    parent_view:    parent.clone(),
                });
            given += usize::from(rel.native_join.is_some());
        }
    }
    given
}

#[cfg(test)]
#[path = "native_joins_tests.rs"]
mod tests;
