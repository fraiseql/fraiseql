//! The expression indexes a filter or sort on a localized field reads (#1513).
//!
//! `compile` reports them and `doctor` checks them against a live database. Each is built by
//! `fraiseql_db::projection_generator::localized_index` over the key the `where` generator and
//! the `ORDER BY` renderer build, so the reported index is the one the planner can match.

use fraiseql_db::projection_generator::{LocalizedIndex, localized_index};

use super::CompiledSchema;

/// One (type, localized field, allowed locale) the schema serves, and the index for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedIndexAdvice {
    /// The type declaring the field.
    pub type_name: String,
    /// The field, as declared.
    pub field:     String,
    /// The allowed locale the index serves.
    pub locale:    String,
    /// The type's source.
    pub table:     String,
    /// The index, when the source is a table (`tv_`/`tb_`). `None` for a view: an index
    /// belongs on its base table, which the schema does not name.
    pub index:     Option<LocalizedIndex>,
}

impl CompiledSchema {
    /// Every index a filter or sort on a localized field reads: one per (type, field,
    /// allowed locale), in declaration order.
    #[must_use]
    pub fn localized_index_report(&self) -> Vec<LocalizedIndexAdvice> {
        let Some(locale) = self.locale.as_ref() else {
            return Vec::new();
        };
        let mut report = Vec::new();
        for type_def in &self.types {
            let table = type_def.sql_source.to_string();
            let relation = table.rsplit('.').next().unwrap_or(&table);
            let table_backed = relation.starts_with("tv_") || relation.starts_with("tb_");
            for field in type_def.fields.iter().filter(|f| f.localized) {
                let key = crate::utils::to_snake_case(field.name.as_str());
                for tag in &locale.allowed {
                    // Derived here rather than read from the load-time cache: `compile`
                    // reports from a schema that has not been loaded.
                    let index = table_backed
                        .then(|| {
                            let chain = locale.derive_chain(tag);
                            let collation = locale.collation(tag);
                            localized_index(&table, &key, tag, &chain, collation.as_deref()).ok()
                        })
                        .flatten();
                    report.push(LocalizedIndexAdvice {
                        type_name: type_def.name.to_string(),
                        field: field.name.to_string(),
                        locale: tag.clone(),
                        table: table.clone(),
                        index,
                    });
                }
            }
        }
        report
    }
}
