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
//! policy is installed, unless something scopes them:
//!
//! * its own `inject_params` (#1800), filters on its denormalized columns that the runtime composes
//!   into every aggregate and window over it, as a list root composes its query's;
//! * failing those, its linked type's own `inject_params`, inherited when each names one of its
//!   denormalized columns. They never come from a query that happens to inject a column of the same
//!   name: scoping is read off a declaration, never off a coincidence;
//! * [`FactRowScope::Relation`](crate::compiler::fact_table::FactRowScope::Relation): its relation
//!   filters its own rows.

use std::fmt;

use indexmap::IndexMap;

use super::{CompiledSchema, type_inject::describe_source};
use crate::{
    compiler::fact_table::{FactRowScope, FactTableMetadata},
    schema::{InjectedParamSource, TypeDefinition},
};

/// Where the `inject_params` scoping a fact table's roots come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactInjectOrigin<'a> {
    /// The fact table's own declaration.
    Declared,
    /// The declaration of the type the fact table is read as, named here.
    Inherited(&'a str),
}

impl fmt::Display for FactInjectOrigin<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Declared => f.write_str("its own inject params"),
            Self::Inherited(type_name) => {
                write!(f, "the inject params inherited from type '{type_name}'")
            },
        }
    }
}

/// The `inject_params` scoping a fact table's aggregate and window roots, and their origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactInjectScope<'a> {
    /// Each denormalized column filtered, and the source of the value it must equal.
    pub params: &'a IndexMap<String, InjectedParamSource>,
    /// Declared on the fact table, or inherited from its type.
    pub origin: FactInjectOrigin<'a>,
}

impl CompiledSchema {
    /// Why the fact table `metadata` names is tenant-scoped with nothing scoping its
    /// aggregate and window roots but a row policy, or `None` when it is not.
    ///
    /// The schema's own registration of the table decides, as for its type link; metadata
    /// handed in (by an embedder, say) can add scoping, never remove it. So a fact table
    /// is tenant-scoped when either the registration or `metadata` makes it so, and it
    /// declares its relation scoped only when the registration does (or, for a table the
    /// schema does not register, `metadata`).
    ///
    /// A table its own or its type's `inject_params` scope ([`Self::fact_table_inject_scope`])
    /// is scoped, read off the same table that decides its row scope. When the type declares
    /// `inject_params` the table cannot take, the reason says why.
    #[must_use]
    pub fn unenforced_tenant_scope(&self, metadata: &FactTableMetadata) -> Option<String> {
        let registered = self.get_fact_table(&metadata.table_name);
        let deciding = registered.unwrap_or(metadata);
        if deciding.row_scope == FactRowScope::Relation
            || self.fact_table_inject_scope(deciding).is_some()
        {
            return None;
        }
        let reason = registered
            .and_then(|table| self.tenant_scope_reason(table))
            .or_else(|| self.tenant_scope_reason(metadata))?;
        let fault = self.fact_table_type(deciding).and_then(|type_def| {
            self.inheritance_fault(deciding, type_def).map(|fault| {
                format!(
                    "; {} cannot scope it: {fault}",
                    FactInjectOrigin::Inherited(type_def.name.as_str())
                )
            })
        });
        Some(reason + fault.as_deref().unwrap_or_default())
    }

    /// The `inject_params` scoping `table`'s aggregate and window roots (#1800): its own, or,
    /// when it declares none, its linked type's, if the table can take each of them. `None` for
    /// a relation-scoped table, which the server does not filter.
    #[must_use]
    pub fn fact_table_inject_scope<'a>(
        &'a self,
        table: &'a FactTableMetadata,
    ) -> Option<FactInjectScope<'a>> {
        if table.row_scope == FactRowScope::Relation {
            return None;
        }
        if !table.inject_params.is_empty() {
            return Some(FactInjectScope {
                params: &table.inject_params,
                origin: FactInjectOrigin::Declared,
            });
        }
        let type_def = self.fact_table_type(table)?;
        if type_def.inject_params.is_empty() || self.inheritance_fault(table, type_def).is_some() {
            return None;
        }
        Some(FactInjectScope {
            params: &type_def.inject_params,
            origin: FactInjectOrigin::Inherited(type_def.name.as_str()),
        })
    }

    /// Declared fact-table `inject_params` the runtime cannot honour as written, one message per
    /// fault, sorted by table: a column that is not one of the table's denormalized filters, an
    /// optional claim, a declaration on a relation-scoped table, and a source contradicting the
    /// linked type's for the same column. An inherited declaration is not concerned: a table
    /// that cannot take it stays unscoped, as before.
    #[must_use]
    pub fn fact_table_inject_violations(&self) -> Vec<String> {
        let mut tables: Vec<&FactTableMetadata> = self.fact_tables.values().collect();
        tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));
        let mut violations = Vec::new();
        for table in tables {
            let name = &table.table_name;
            if table.inject_params.is_empty() {
                continue;
            }
            if table.row_scope == FactRowScope::Relation {
                violations.push(format!(
                    "fact table '{name}' declares row_scope = \"relation\" (its relation filters \
                     its own rows) and inject_params (the server filters them): declare one"
                ));
            }
            let linked = self.fact_table_type(table);
            for (column, source) in &table.inject_params {
                if let Some(fault) = column_fault(table, column, source) {
                    violations.push(format!("fact table '{name}': inject param {fault}"));
                }
                let Some(type_def) = linked else { continue };
                if let Some(type_source) = type_def.inject_params.get(column) {
                    if type_source != source {
                        violations.push(format!(
                            "fact table '{name}' and type '{}' both scope '{column}', from \
                             different sources: the fact table from {}, the type from {}. Give \
                             them the same source, or drop the fact table's.",
                            type_def.name,
                            describe_source(source),
                            describe_source(type_source),
                        ));
                    }
                }
            }
        }
        violations
    }

    /// Why `table` cannot take `type_def`'s `inject_params`, or `None` when it can.
    fn inheritance_fault(
        &self,
        table: &FactTableMetadata,
        type_def: &TypeDefinition,
    ) -> Option<String> {
        type_def
            .inject_params
            .iter()
            .find_map(|(column, source)| column_fault(table, column, source))
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

/// Why the inject param `column` from `source` cannot filter `table`'s rows, or `None`: it must
/// name one of the table's denormalized filter columns, and an absent claim cannot filter.
fn column_fault(
    table: &FactTableMetadata,
    column: &str,
    source: &InjectedParamSource,
) -> Option<String> {
    if !table.denormalized_filters.iter().any(|filter| filter.name == column) {
        let mut filters: Vec<String> =
            table.denormalized_filters.iter().map(|f| format!("'{}'", f.name)).collect();
        filters.sort();
        return Some(format!(
            "'{column}' is not one of its denormalized filter columns ({})",
            if filters.is_empty() {
                "it has none".to_string()
            } else {
                filters.join(", ")
            }
        ));
    }
    if let InjectedParamSource::JwtOptional(claim) = source {
        return Some(format!(
            "'{column}' is 'jwt_optional:{claim}', but it filters rows, and an absent claim \
             cannot: use 'jwt:{claim}'"
        ));
    }
    None
}
