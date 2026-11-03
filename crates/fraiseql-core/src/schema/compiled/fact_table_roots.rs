//! The roots a fact table publishes: `<name>_aggregate` and `<name>_window` (#1800).
//!
//! The dispatch routes a query root by that suffix, before any declared query is looked up, and
//! reads the fact table registered as `tf_<name>`. A schema in which a root cannot reach its
//! table, or a declared query answers to a root's name, is refused at load, naming each one.

use super::CompiledSchema;

/// Whether a query root is a fact table's aggregate or window root.
#[must_use]
pub fn is_fact_table_root(name: &str) -> bool {
    name.ends_with("_aggregate") || name.ends_with("_window")
}

impl CompiledSchema {
    /// The published fact-table root named `name`. `None` until the derived surface is built
    /// ([`build_indexes`](Self::build_indexes)), and for a name no fact table publishes.
    #[must_use]
    pub fn fact_root(&self, name: &str) -> Option<&crate::schema::fact_table_surface::FactRoot> {
        self.derived.fact_roots.iter().find(|root| root.name == name)
    }

    /// Declarations the fact-table roots cannot be reached through, one message each:
    ///
    /// - a declared query named like a root, which the dispatch never reaches;
    /// - a fact table not named `tf_<name>`, whose roots name no table;
    /// - a fact table registered under one name and naming another table, whose roots would answer
    ///   with that other table's rows.
    pub(super) fn fact_table_root_violations(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .queries
            .iter()
            .filter(|q| is_fact_table_root(&q.name))
            .map(|q| {
                format!(
                    "query `{}` is named like a fact table's root (`_aggregate`, `_window`), \
                     which the dispatch routes to a fact table: rename it and recompile",
                    q.name
                )
            })
            .collect();
        let mut tables: Vec<(&String, &String)> =
            self.fact_tables.iter().map(|(key, t)| (key, &t.table_name)).collect();
        tables.sort();
        for (key, table_name) in tables {
            if key != table_name {
                out.push(format!(
                    "fact table registered as `{key}` names table `{table_name}`: register it \
                     under its table name and recompile"
                ));
            } else if !key.starts_with("tf_") {
                out.push(format!(
                    "fact table `{key}` is not named `tf_<name>`, so no `<name>_aggregate` or \
                     `<name>_window` root reaches it: rename it and recompile"
                ));
            }
        }
        out
    }
}

impl CompiledSchema {
    /// Declarations the fact-table roots cannot be published with (#1800), one message each:
    /// a key that is not a GraphQL name, a row field two sources produce, and a declared
    /// type, input or enum taking a name the roots publish as something else.
    pub(super) fn fact_table_surface_violations(
        &self,
        tables: &[&crate::compiler::fact_table::FactTableMetadata],
    ) -> Vec<String> {
        use crate::schema::fact_table_surface::{
            derive, derived_names, row_field_collisions, unpublishable_key_violations,
        };

        let mut out: Vec<String> = tables
            .iter()
            .flat_map(|t| {
                unpublishable_key_violations(t).into_iter().chain(row_field_collisions(t))
            })
            .collect();
        let surface = derive(self);
        for (name, kind) in derived_names(&surface) {
            let declared = match kind {
                "object" => {
                    self.find_type(name).is_some()
                        || self.find_input_type(name).is_some()
                        || self.find_enum(name).is_some()
                        || self.interfaces.iter().any(|i| i.name == name)
                        || self.unions.iter().any(|u| u.name == name)
                },
                "input" => {
                    self.find_type(name).is_some()
                        || self
                            .find_input_type(name)
                            .is_some_and(|i| surface.input_types.iter().all(|d| d != i))
                },
                _ => {
                    // A declared `SortDirection` serves if it holds the two directions.
                    let differs = |e: &crate::schema::EnumDefinition| {
                        if name == crate::schema::derived_inputs::SORT_DIRECTION_ENUM {
                            let values: Vec<&str> =
                                e.values.iter().map(|v| v.name.as_str()).collect();
                            values != ["ASC", "DESC"]
                        } else {
                            surface.enums.iter().all(|d| d != e)
                        }
                    };
                    self.find_type(name).is_some()
                        || self.find_input_type(name).is_some()
                        || self.find_enum(name).is_some_and(differs)
                },
            };
            if declared {
                let owner = surface
                    .roots
                    .iter()
                    .find(|r| r.row_type.as_deref() == Some(name))
                    .or_else(|| surface.roots.first())
                    .map_or("", |r| r.fact_table.as_str());
                out.push(format!(
                    "`{name}` is declared, and the roots of fact table `{owner}` publish the \
                     {kind} of that name: rename the declaration and recompile"
                ));
            }
        }
        out
    }
}
