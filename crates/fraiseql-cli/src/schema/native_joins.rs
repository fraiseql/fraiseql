//! The native columns a relationship joins on, proven by the database (#1587).
//!
//! A composed read correlates an embedded level to its parent by stored document keys unless
//! the compiled schema says which SQL columns join them. `fraiseql compile --database` reads
//! that from pg_tviews' registry: a relationship whose child and parent types read TVIEWs,
//! where the child TVIEW's reference column holds the parent TVIEW's identity, proven by a
//! `FOREIGN KEY` (`tviews.registry.references`). Nothing is guessed from a name: a type that
//! reads a plain view, or a pair the registry does not link, keeps its stored keys.

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
/// Of several references from the child to the parent, the one named by the relationship's
/// `foreign_key`; else the only one; two or more and none named: no proof. Returns how many
/// relationships were given one.
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
            let candidates: Vec<&TviewReference> =
                references.iter().filter(|r| &r.child == child && &r.parent == parent).collect();
            let only = match candidates.as_slice() {
                [only] => Some(only),
                _ => None,
            };
            let chosen = candidates.iter().find(|r| r.foreign_key == rel.foreign_key).or(only);
            rel.native_join = chosen.map(|r| NativeJoin {
                foreign_key:    r.foreign_key.clone(),
                referenced_key: r.referenced_key.clone(),
            });
            given += usize::from(rel.native_join.is_some());
        }
    }
    given
}

#[cfg(test)]
#[path = "native_joins_tests.rs"]
mod tests;
