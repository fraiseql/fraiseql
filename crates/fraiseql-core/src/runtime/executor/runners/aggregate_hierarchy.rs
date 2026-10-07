//! Node-id filters on a fact table's path column (#1498).
//!
//! `descendant_of_id` / `ancestor_of_id` take a node id and compare against that node's
//! path, read from the hierarchy the column declares (`FilterColumn::hierarchy`, naming a
//! `[hierarchies.<name>]` table). The aggregate parser knows the fact table but not the
//! compiled schema's hierarchies, so this step attaches them: it wraps each such leaf in
//! `WhereClause::InHierarchy`, the shape the main `where` parser builds (#1396), and the
//! aggregate SQL generator lowers it to a path subquery.

use fraiseql_db::where_generator::HierarchyContext;

use crate::{
    backend::{WhereClause, WhereOperator},
    compiler::fact_table::FactTableMetadata,
    error::{FraiseQLError, Result},
    schema::HierarchiesConfig,
};

/// Wrap every node-id filter in `clause` in the hierarchy its column declares.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] for a node-id filter on a column that is not a
/// denormalized filter, declares no hierarchy, or names one the schema does not declare.
pub(super) fn attach_hierarchies(
    clause: WhereClause,
    metadata: &FactTableMetadata,
    hierarchies: Option<&HierarchiesConfig>,
) -> Result<WhereClause> {
    let attach = |c| attach_hierarchies(c, metadata, hierarchies);
    Ok(match clause {
        WhereClause::And(clauses) => {
            WhereClause::And(clauses.into_iter().map(attach).collect::<Result<_>>()?)
        },
        WhereClause::Or(clauses) => {
            WhereClause::Or(clauses.into_iter().map(attach).collect::<Result<_>>()?)
        },
        WhereClause::Not(inner) => WhereClause::Not(Box::new(attach(*inner)?)),
        WhereClause::Typed { types, inner } => WhereClause::Typed {
            types,
            inner: Box::new(attach(*inner)?),
        },
        leaf => match node_id_column(&leaf) {
            Some(column) => WhereClause::InHierarchy {
                context: context_for(column, metadata, hierarchies)?,
                inner:   Box::new(leaf),
            },
            None => leaf,
        },
    })
}

/// The column a node-id filter leaf compares, or `None` for any other clause.
fn node_id_column(leaf: &WhereClause) -> Option<&str> {
    let is_id = |op: &WhereOperator| {
        matches!(op, WhereOperator::DescendantOfId | WhereOperator::AncestorOfId)
    };
    match leaf {
        WhereClause::NativeField {
            column, operator, ..
        } if is_id(operator) => Some(column),
        WhereClause::Field { path, operator, .. } if is_id(operator) => {
            path.first().map(String::as_str)
        },
        _ => None,
    }
}

fn context_for(
    column: &str,
    metadata: &FactTableMetadata,
    hierarchies: Option<&HierarchiesConfig>,
) -> Result<HierarchyContext> {
    let refuse = |why: String| {
        Err(FraiseQLError::validation(format!(
            "descendant_of_id / ancestor_of_id on '{column}' of fact table '{}': {why}",
            metadata.table_name
        )))
    };
    let Some(filter) = metadata.denormalized_filters.iter().find(|f| f.name == column) else {
        return refuse(
            "a node id resolves only on an ltree denormalized filter column that declares \
             its hierarchy; filter a path with descendant_of / ancestor_of"
                .to_string(),
        );
    };
    let Some(name) = &filter.hierarchy else {
        return refuse(
            "the column declares no hierarchy; give the denormalized filter \
             `hierarchy = \"<name>\"` naming a [hierarchies.<name>] table, or filter the \
             path with descendant_of / ancestor_of"
                .to_string(),
        );
    };
    let Some(definition) = hierarchies.and_then(|h| h.get(name)) else {
        return refuse(format!("hierarchy '{name}' is not declared in [hierarchies]"));
    };
    Ok(HierarchyContext {
        table:       definition.table.clone(),
        path_column: definition.path_column.clone(),
        fk_column:   None,
    })
}
