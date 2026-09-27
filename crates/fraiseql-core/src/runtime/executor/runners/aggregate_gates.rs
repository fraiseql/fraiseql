//! Read gates of an aggregate or a window over a fact table linked to a type (ruling AB 2).
//!
//! A linked fact table is read as its type. Every name a request references — grouped by,
//! aggregated, filtered, ordered, partitioned, selected, or read by a window function — is a
//! reference in the sense of ruling AA 3, and the response is made of nothing else: a group
//! key is the value itself, an aggregate is computed from the values. So gating the
//! references gates the output. Each name must be a field of the type (an undeclared JSONB
//! key is no field at all), every field must pass
//! [`can_reference_field`](crate::runtime::can_reference_field), and the type's own
//! `requires_role` must be held.
//!
//! Checked on the parsed request, before the row policy is composed into it, so the
//! policy's own predicate is never classified.

use crate::{
    compiler::{
        aggregation::{AggregateSelection, AggregationRequest, GroupBySelection},
        fact_table::FactTableMetadata,
        window_functions::{
            PartitionByColumn, WindowFunctionSpec, WindowRequest, WindowSelectColumn,
        },
    },
    db::WhereClause,
    error::{FraiseQLError, Result},
    schema::{CompiledSchema, TypeDefinition, fact_field},
    security::SecurityContext,
};

/// The type `metadata` is read as. The schema's own registration of the table decides; the
/// metadata handed in (by an embedder, say) can only add a link, never remove one.
fn linked_type<'a>(
    schema: &'a CompiledSchema,
    metadata: &FactTableMetadata,
) -> Option<&'a TypeDefinition> {
    schema
        .get_fact_table(&metadata.table_name)
        .and_then(|registered| schema.fact_table_type(registered))
        .or_else(|| schema.fact_table_type(metadata))
}

/// The names one request references, with what it does with each.
struct References<'r> {
    names: Vec<(&'r str, &'static str)>,
}

impl<'r> References<'r> {
    const fn new() -> Self {
        Self { names: Vec::new() }
    }

    fn add(&mut self, name: &'r str, usage: &'static str) {
        self.names.push((name, usage));
    }

    fn add_where(&mut self, clause: &'r WhereClause) -> Result<()> {
        match clause {
            WhereClause::And(clauses) | WhereClause::Or(clauses) => {
                clauses.iter().try_for_each(|c| self.add_where(c))
            },
            WhereClause::Not(inner) | WhereClause::Typed { inner, .. } => self.add_where(inner),
            WhereClause::Field { path, .. } => {
                // One key: a fact table's filter is flat (`data->>'key'`).
                path.first().map_or(Ok(()), |key| {
                    self.add(key, "filter by");
                    Ok(())
                })
            },
            WhereClause::NativeField { column, .. } => {
                self.add(column, "filter by");
                Ok(())
            },
            // `WhereClause` is non-exhaustive; the aggregate and window parsers build none
            // other. One that cannot be classified is refused, never passed through.
            _ => Err(FraiseQLError::Validation {
                message: "unsupported condition in an aggregate or window filter".to_string(),
                path:    None,
            }),
        }
    }

    fn add_aggregate(&mut self, aggregate: &'r AggregateSelection) {
        match aggregate {
            AggregateSelection::CountDistinct { field, .. }
            | AggregateSelection::BoolAggregate { field, .. } => self.add(field, "aggregate"),
            AggregateSelection::MeasureAggregate { measure, .. } => self.add(measure, "aggregate"),
            // `count` reads no field. The match is exhaustive on purpose: a variant the parser
            // grows does not compile until it is classified here.
            AggregateSelection::Count { .. } => {},
        }
    }
}

/// The declared name a group-by selection reads: a native mapping's column goes back to the
/// dimension key it maps.
fn group_by_name<'m>(selection: &'m GroupBySelection, metadata: &'m FactTableMetadata) -> &'m str {
    match selection {
        GroupBySelection::Dimension { path, .. } => path,
        GroupBySelection::TemporalBucket { column, .. } => column,
        GroupBySelection::CalendarDimension { source_column, .. } => source_column,
        GroupBySelection::NativeDimension { column, .. } => metadata
            .native_dimension_mapping
            .iter()
            .find(|(_, mapped)| *mapped == column)
            .map_or(column.as_str(), |(key, _)| key.as_str()),
    }
}

/// Refuse an aggregate request over a linked fact table that references a field the caller
/// may not read, a name its type does not declare, or a type whose role the caller lacks.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] for an undeclared name; [`FraiseQLError::Authorization`]
/// for an unreadable field or the type's role.
pub(super) fn refuse_unreadable_aggregate(
    schema: &CompiledSchema,
    metadata: &FactTableMetadata,
    request: &AggregationRequest,
    security_context: Option<&SecurityContext>,
) -> Result<()> {
    let Some(type_def) = linked_type(schema, metadata) else {
        return Ok(());
    };
    let mut refs = References::new();
    if let Some(clause) = &request.where_clause {
        refs.add_where(clause)?;
    }
    for selection in &request.group_by {
        refs.add(group_by_name(selection, metadata), "group by");
    }
    for aggregate in &request.aggregates {
        refs.add_aggregate(aggregate);
    }
    // `having` adds no reference: the parser admits a condition only on a selected aggregate,
    // and those are the references above.
    let outputs: Vec<&str> = request
        .group_by
        .iter()
        .map(GroupBySelection::alias)
        .chain(request.aggregates.iter().map(AggregateSelection::alias))
        .collect();
    for clause in &request.order_by {
        if !outputs.contains(&clause.field.as_str()) {
            refs.add(&clause.field, "order by");
        }
    }
    refuse(schema, metadata, type_def, &refs, security_context)
}

/// [`refuse_unreadable_aggregate`] for a window request.
///
/// # Errors
///
/// As [`refuse_unreadable_aggregate`].
pub(super) fn refuse_unreadable_window(
    schema: &CompiledSchema,
    metadata: &FactTableMetadata,
    request: &WindowRequest,
    security_context: Option<&SecurityContext>,
) -> Result<()> {
    let Some(type_def) = linked_type(schema, metadata) else {
        return Ok(());
    };
    let mut refs = References::new();
    if let Some(clause) = &request.where_clause {
        refs.add_where(clause)?;
    }
    for column in &request.select {
        match column {
            WindowSelectColumn::Measure { name, .. } | WindowSelectColumn::Filter { name, .. } => {
                refs.add(name, "select");
            },
            WindowSelectColumn::Dimension { path, .. } => refs.add(path, "select"),
        }
    }
    let outputs: Vec<&str> = request
        .select
        .iter()
        .map(WindowSelectColumn::alias)
        .chain(request.windows.iter().map(|w| w.alias.as_str()))
        .collect();
    for window in &request.windows {
        match &window.function {
            WindowFunctionSpec::Lag { field, .. }
            | WindowFunctionSpec::Lead { field, .. }
            | WindowFunctionSpec::FirstValue { field }
            | WindowFunctionSpec::LastValue { field }
            | WindowFunctionSpec::NthValue { field, .. }
            | WindowFunctionSpec::RunningCountField { field } => refs.add(field, "window over"),
            WindowFunctionSpec::RunningSum { measure }
            | WindowFunctionSpec::RunningAvg { measure }
            | WindowFunctionSpec::RunningMin { measure }
            | WindowFunctionSpec::RunningMax { measure }
            | WindowFunctionSpec::RunningStddev { measure }
            | WindowFunctionSpec::RunningVariance { measure } => refs.add(measure, "window over"),
            WindowFunctionSpec::RowNumber
            | WindowFunctionSpec::Rank
            | WindowFunctionSpec::DenseRank
            | WindowFunctionSpec::Ntile { .. }
            | WindowFunctionSpec::PercentRank
            | WindowFunctionSpec::CumeDist
            | WindowFunctionSpec::RunningCount => {},
        }
        for column in &window.partition_by {
            match column {
                PartitionByColumn::Dimension { path } => refs.add(path, "partition by"),
                PartitionByColumn::Filter { name } | PartitionByColumn::Measure { name } => {
                    refs.add(name, "partition by");
                },
            }
        }
        for order in &window.order_by {
            if !outputs.contains(&order.field.as_str()) {
                refs.add(&order.field, "order by");
            }
        }
    }
    for order in &request.order_by {
        if !outputs.contains(&order.field.as_str()) {
            refs.add(&order.field, "order by");
        }
    }
    refuse(schema, metadata, type_def, &refs, security_context)
}

fn refuse(
    schema: &CompiledSchema,
    metadata: &FactTableMetadata,
    type_def: &TypeDefinition,
    refs: &References<'_>,
    security_context: Option<&SecurityContext>,
) -> Result<()> {
    let type_name = type_def.name.as_str();
    if let Some(role) = type_def.requires_role.as_deref() {
        if !security_context.is_some_and(|ctx| ctx.roles.iter().any(|r| r == role)) {
            return Err(FraiseQLError::Authorization {
                message:  format!(
                    "Access denied: fact table '{}' is read as '{type_name}', whose read \
                     requires a role the request does not hold",
                    metadata.table_name
                ),
                action:   Some("read".to_string()),
                resource: Some(type_name.to_string()),
            });
        }
    }
    for &(name, usage) in &refs.names {
        let Some(field) = fact_field(type_def, name) else {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "'{name}' is not a field of '{type_name}', which fact table '{}' is read as",
                    metadata.table_name
                ),
                path:    None,
            });
        };
        super::super::support::security::refuse_unreadable_reference(
            schema,
            type_name,
            field,
            security_context,
            usage,
        )?;
    }
    Ok(())
}
