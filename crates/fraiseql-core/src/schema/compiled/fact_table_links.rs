//! The fact-table → type link: refusal of a link the runtime cannot enforce (ruling AB 1).
//!
//! A fact table linked to a type (`FactTableMetadata::type_name`) is read as that type: an
//! aggregate or a window over it references its measures, filters and dimension paths as
//! fields of the type, and each reference is gated as a read of that field. A declared column
//! the type does not have would be a column no gate can reach, so the link must be complete —
//! and the type must exist. Refused at load, whatever produced the schema, naming the fact
//! table, the type and the names.

use super::CompiledSchema;
use crate::{
    compiler::fact_table::FactTableMetadata,
    schema::{FieldDefinition, TypeDefinition},
};

/// The field of `type_def` a fact-table name refers to: matched on its `snake_case` form, as a
/// query filter's storage key is (`support::security`), so `organizationId` and
/// `organization_id` are one field.
pub fn fact_field<'a>(type_def: &'a TypeDefinition, name: &str) -> Option<&'a FieldDefinition> {
    let key = crate::utils::to_snake_case(name);
    type_def
        .fields
        .iter()
        .find(|f| crate::utils::to_snake_case(f.name.as_str()) == key)
}

/// Where a fact-table name reads: the first key of its dimension path, the key it is
/// declared at whatever it is called; the name itself for any other reference.
fn first_key(metadata: &FactTableMetadata, name: &str) -> String {
    let key = crate::utils::to_snake_case(name);
    metadata
        .dimensions
        .paths
        .iter()
        .find(|p| p.name == name || crate::utils::to_snake_case(&p.name) == key)
        .and_then(|p| p.segments(&metadata.dimensions.name).ok())
        .and_then(|segments| segments.into_iter().next())
        .unwrap_or(key)
}

/// Every name a fact table declares: what a request over it can reference.
fn declared_names(metadata: &FactTableMetadata) -> Vec<&str> {
    let mut names: Vec<&str> = metadata
        .measures
        .iter()
        .map(|m| m.name.as_str())
        .chain(metadata.denormalized_filters.iter().map(|f| f.name.as_str()))
        .chain(metadata.dimensions.paths.iter().map(|p| p.name.as_str()))
        .chain(metadata.calendar_dimensions.iter().map(|c| c.source_column.as_str()))
        .chain(metadata.native_dimension_mapping.keys().map(String::as_str))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

impl CompiledSchema {
    /// The scoped type a fact-table name reads into, when it reads into one: the name's
    /// first key (of its dimension path, wherever it is declared and whatever it is called;
    /// the name itself otherwise) is a field of the linked type `type_def` that holds a type
    /// scoped by `inject_params` ([`type_scope`](Self::type_scope)). Returned as the field
    /// and the scoped type.
    ///
    /// A group key, a selected value or an aggregate is a value, not a row, so no scope
    /// filters it: such a name is refused when the schema loads and when a request names it.
    #[must_use]
    pub fn fact_name_in_scoped_type<'a>(
        &'a self,
        metadata: &FactTableMetadata,
        type_def: &'a TypeDefinition,
        name: &str,
    ) -> Option<(&'a str, &'a str)> {
        let field = fact_field(type_def, &first_key(metadata, name))?;
        let (scoped, _) = self.scoped_targets(&field.field_type).into_iter().next()?;
        Some((field.name.as_str(), scoped))
    }

    /// Fact tables declaring a name that reads into a type scoped by `inject_params`,
    /// through a field of the type they are read as. One message per name, empty when none
    /// does; unlinked fact tables are not concerned.
    #[must_use]
    pub fn fact_table_scoped_type_violations(&self) -> Vec<String> {
        let mut tables: Vec<&FactTableMetadata> = self.fact_tables.values().collect();
        tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));
        let mut out = Vec::new();
        for metadata in tables {
            let Some(type_def) = self.fact_table_type(metadata) else {
                continue;
            };
            for name in declared_names(metadata) {
                if let Some((field, scoped)) =
                    self.fact_name_in_scoped_type(metadata, type_def, name)
                {
                    out.push(format!(
                        "fact table '{}' declares `{name}`, which reads inside `{}.{field}`, a \
                         `{scoped}`: `{scoped}` is scoped by `inject_params`, and a value an \
                         aggregate or a window serves is not a row its scope can filter. Read \
                         `{scoped}` through a query of its own, or drop `{name}` from the fact \
                         table.",
                        metadata.table_name, type_def.name
                    ));
                }
            }
        }
        out
    }

    /// Fact tables that declare one dimension twice, in ways a request could read two
    /// ways (#1231): a `native_dimension_mapping` key that is also a
    /// `denormalized_filters` column mapped to a different column, or two mapping keys
    /// naming one dimension (in two casings) mapped to different columns. One message
    /// per conflict, empty when every declaration agrees.
    #[must_use]
    pub fn fact_table_mapping_violations(&self) -> Vec<String> {
        use crate::compiler::fact_table::dimension_key;

        let mut tables: Vec<&FactTableMetadata> = self.fact_tables.values().collect();
        tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));
        let mut out = Vec::new();
        for table in tables {
            let mut mapping: Vec<(&String, &String)> =
                table.native_dimension_mapping.iter().collect();
            mapping.sort();
            for (i, (key, column)) in mapping.iter().enumerate() {
                let dimension = dimension_key(key);
                if let Some(filter) = table
                    .denormalized_filters
                    .iter()
                    .find(|f| dimension_key(&f.name) == dimension && f.name != **column)
                {
                    out.push(format!(
                        "fact table `{}`: native_dimension_mapping maps `{key}` to column \
                         `{column}`, and denormalized_filters declares `{}` as a column of \
                         its own; declare the dimension once",
                        table.table_name, filter.name
                    ));
                }
                for (other, other_column) in &mapping[i + 1..] {
                    if dimension_key(other) == dimension && other_column != column {
                        out.push(format!(
                            "fact table `{}`: native_dimension_mapping maps `{key}` to \
                             `{column}` and `{other}` to `{other_column}`, which name one \
                             dimension; declare it once",
                            table.table_name
                        ));
                    }
                }
            }
        }
        out
    }

    /// `native_dimension_mapping` values that name no declared `denormalized_filters`
    /// column (#1517).
    ///
    /// A mapped dimension is read from its column by `groupBy` and `where`, and a filter on
    /// it binds with the column's declared type. A column the fact table does not declare
    /// has no type, and nothing says it exists: the mapping is refused, naming where to
    /// declare it.
    #[must_use]
    pub fn fact_table_mapping_column_violations(&self) -> Vec<String> {
        let mut tables: Vec<&FactTableMetadata> = self.fact_tables.values().collect();
        tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));
        let mut out = Vec::new();
        for table in tables {
            let mut mapping: Vec<(&String, &String)> =
                table.native_dimension_mapping.iter().collect();
            mapping.sort();
            for (key, column) in mapping {
                if !table.denormalized_filters.iter().any(|f| f.name == *column) {
                    out.push(format!(
                        "fact table `{}`: native_dimension_mapping maps `{key}` to column \
                         `{column}`, which denormalized_filters does not declare; declare it \
                         there (with its sql_type)",
                        table.table_name
                    ));
                }
            }
        }
        out
    }

    /// Declared dimension paths whose `json_path` the runtime cannot read (#1517).
    ///
    /// `groupBy` and `where` read a declared path at the keys its `json_path` names
    /// ([`DimensionPath::segments`](crate::compiler::fact_table::DimensionPath::segments)),
    /// so a path of another shape is refused here rather than failing every request that
    /// names it.
    #[must_use]
    pub fn fact_table_path_violations(&self) -> Vec<String> {
        let mut tables: Vec<&FactTableMetadata> = self.fact_tables.values().collect();
        tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));
        tables
            .into_iter()
            .flat_map(|table| {
                table.dimensions.paths.iter().filter_map(|path| {
                    path.segments(&table.dimensions.name)
                        .err()
                        .map(|e| format!("fact table `{}`: {e}", table.table_name))
                })
            })
            .collect()
    }

    /// The type a fact table is read as, when it is linked to one that exists.
    #[must_use]
    pub fn fact_table_type(&self, metadata: &FactTableMetadata) -> Option<&TypeDefinition> {
        let name = metadata.type_name.as_deref()?;
        self.types.iter().find(|t| t.name == name)
    }

    /// Fact tables whose link to a type cannot be enforced: the type does not exist, or it
    /// lacks a name the fact table declares. One message per fact table, empty when every
    /// link is enforceable; unlinked fact tables are not concerned.
    #[must_use]
    pub fn fact_table_link_violations(&self) -> Vec<String> {
        let mut tables: Vec<&FactTableMetadata> = self.fact_tables.values().collect();
        tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));
        tables
            .into_iter()
            .filter_map(|metadata| {
                let type_name = metadata.type_name.as_deref()?;
                let Some(type_def) = self.fact_table_type(metadata) else {
                    return Some(format!(
                        "fact table '{}' is read as type '{type_name}', which the schema does \
                         not declare",
                        metadata.table_name
                    ));
                };
                let missing: Vec<&str> = declared_names(metadata)
                    .into_iter()
                    .filter(|name| fact_field(type_def, name).is_none())
                    .collect();
                (!missing.is_empty()).then(|| {
                    format!(
                        "fact table '{}' is read as type '{type_name}', which has no field for \
                         {}: a column the type does not declare is one no read gate can reach. \
                         Declare the fields on '{type_name}', or drop them from the fact table.",
                        metadata.table_name,
                        missing.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ")
                    )
                })
            })
            .collect()
    }
}
