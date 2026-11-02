//! Tenant scoping of a fact table's aggregate and window roots (#1799).
//!
//! An aggregate or a window root has no `QueryDefinition`, hence no `inject_params`: the
//! tenant predicate a list root composes from its own declaration has nowhere to come from.
//! The runtime scopes these roots only through a row policy, which the server does not
//! install. So in a schema that scopes its reads by `inject_params`, a fact table holding
//! the same tenants' rows was read across all of them.
//!
//! A fact table is tenant-scoped when the schema says its rows belong to a tenant:
//!
//! * its linked type (`type_name`) declares `inject_params`, or is returned by a query that does —
//!   it is read as that type, and that type is read per tenant;
//! * one of its denormalized filter columns is a column some query or type injects — the column the
//!   scoped reads filter on is in the fact table too.
//!
//! Such a fact table is refused by the compiler, and its roots by the runtime when no row
//! policy is installed, unless it declares
//! [`FactRowScope::Relation`](crate::compiler::fact_table::FactRowScope::Relation): its
//! relation filters its own rows.

use super::CompiledSchema;
use crate::compiler::fact_table::{FactRowScope, FactTableMetadata};

impl CompiledSchema {
    /// Why the fact table `metadata` names is tenant-scoped with nothing scoping its
    /// aggregate and window roots but a row policy, or `None` when it is not.
    ///
    /// The schema's own registration of the table decides, as for its type link; metadata
    /// handed in (by an embedder, say) can add scoping, never remove it. So a fact table
    /// is tenant-scoped when either the registration or `metadata` makes it so, and it
    /// declares its relation scoped only when the registration does (or, for a table the
    /// schema does not register, `metadata`).
    #[must_use]
    pub fn unenforced_tenant_scope(&self, metadata: &FactTableMetadata) -> Option<String> {
        let registered = self.get_fact_table(&metadata.table_name);
        if registered.unwrap_or(metadata).row_scope == FactRowScope::Relation {
            return None;
        }
        registered
            .and_then(|table| self.tenant_scope_reason(table))
            .or_else(|| self.tenant_scope_reason(metadata))
    }

    /// The registered fact tables [`unenforced_tenant_scope`](Self::unenforced_tenant_scope)
    /// finds unscoped, one message per table, sorted by name.
    #[must_use]
    pub fn fact_table_scope_violations(&self) -> Vec<String> {
        let mut tables: Vec<&FactTableMetadata> = self.fact_tables.values().collect();
        tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));
        tables
            .into_iter()
            .filter_map(|table| {
                self.unenforced_tenant_scope(table)
                    .map(|reason| format!("fact table '{}' ({reason})", table.table_name))
            })
            .collect()
    }

    /// Why `table` is tenant-scoped in this schema, whatever its declared row scope.
    fn tenant_scope_reason(&self, table: &FactTableMetadata) -> Option<String> {
        self.linked_type_reason(table).or_else(|| self.injected_column_reason(table))
    }

    fn linked_type_reason(&self, table: &FactTableMetadata) -> Option<String> {
        let type_def = self.fact_table_type(table)?;
        if let Some(column) = type_def.inject_params.keys().next() {
            return Some(format!(
                "it is read as type '{}', which injects '{column}'",
                type_def.name
            ));
        }
        self.queries
            .iter()
            .filter(|q| q.return_type == type_def.name)
            .find_map(|q| q.inject_params.keys().next().map(|column| (q, column)))
            .map(|(query, column)| {
                format!(
                    "it is read as type '{}', which query '{}' reads injecting '{column}'",
                    type_def.name, query.name
                )
            })
    }

    fn injected_column_reason(&self, table: &FactTableMetadata) -> Option<String> {
        table.denormalized_filters.iter().find_map(|filter| {
            let column = filter.name.as_str();
            self.queries
                .iter()
                .find(|q| q.inject_params.contains_key(column))
                .map(|q| format!("its column '{column}' is injected by query '{}'", q.name))
                .or_else(|| {
                    self.types
                        .iter()
                        .find(|t| t.inject_params.contains_key(column))
                        .map(|t| format!("its column '{column}' is injected by type '{}'", t.name))
                })
        })
    }
}
