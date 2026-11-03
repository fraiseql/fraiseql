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
