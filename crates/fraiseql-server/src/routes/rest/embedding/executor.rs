//! Embedding executor: join key extraction, relationship traversal, and data merging.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, QueryMatch},
    schema::{Cardinality, CompiledSchema, Relationship, RestConfig},
    security::SecurityContext,
};

use crate::routes::rest::handler::RestError;

/// Shared context for embedding execution, reducing argument count.
pub(super) struct EmbedCtx<'a, A: DatabaseAdapter> {
    pub executor:         &'a Arc<Executor<A>>,
    pub schema:           &'a CompiledSchema,
    pub config:           &'a RestConfig,
    /// The type whose rows are being embedded *into*.
    ///
    /// Carried so the join key can be read off a parent row in the spelling that
    /// row actually uses — see [`declared_key`]. Without it `extract_join_key`
    /// had only the relationship's storage column to go on.
    pub parent_type:      &'a str,
    pub security_context: Option<&'a SecurityContext>,
}

/// The spelling `type_name` publishes for the storage column `column`.
///
/// A relationship's `foreign_key`/`referenced_key` are SQL **column** names
/// (`fk_author`, per `[[relationships]]` in `fraiseql.toml`), but neither side of
/// an embed speaks storage:
///
/// * the join predicate is handed to the same `where` parser a client's filter goes through, and
///   since 2.15.0 that parser accepts only the name the schema *declares*;
/// * the parent row it is scoped by has already been projected, and a projected row is keyed by
///   [`FieldDefinition::name`](fraiseql_core::schema::FieldDefinition) — the declared name — too.
///
/// Under `naming_convention = "camelCase"` the declared name is `fkAuthor`. Passing
/// the raw column through would make the server refuse its own parent-scoping
/// predicate on one side and fail to find the join value on the other; both
/// collapse the embed silently, on a 200.
///
/// This is the same rule `build_fts_where_clause` already follows by keying off
/// `f.name`: anything the server composes against the published surface speaks that
/// surface, and the lowering back to storage happens once, inside the parser.
///
/// Falls back to the column as written when the schema cannot name the type or
/// declares no field matching it — that level is unadjudicated anyway (#939),
/// so passing it through is strictly better than inventing a spelling.
pub(super) fn declared_key(schema: &CompiledSchema, type_name: &str, column: &str) -> String {
    let storage = fraiseql_core::utils::to_snake_case(column);
    schema
        .find_type(type_name)
        .and_then(|td| {
            td.fields
                .iter()
                .find(|f| fraiseql_core::utils::to_snake_case(f.name.as_str()) == storage)
                .map(|f| f.name.to_string())
        })
        .unwrap_or_else(|| column.to_string())
}

/// The column an embed of `rel` reads off the **parent** row.
///
/// `OneToMany` hangs the foreign key on the child, so the parent side is its
/// `referenced_key` — conventionally `id`, which a client selects anyway, which is
/// why #1230 hid here for so long. `ManyToOne`/`OneToOne` hold the key themselves,
/// so the parent side is the `foreign_key` — the one column a client asking for
/// `author` has no reason to select.
pub(super) const fn parent_join_column(rel: &Relationship) -> &String {
    match rel.cardinality {
        Cardinality::ManyToOne | Cardinality::OneToOne => &rel.foreign_key,
        _ => &rel.referenced_key,
    }
}

/// The column the join predicate filters on the **target** row — the mirror of
/// [`parent_join_column`], stated once so the two can never drift apart.
pub(super) const fn target_join_column(rel: &Relationship) -> &String {
    match rel.cardinality {
        Cardinality::ManyToOne | Cardinality::OneToOne => &rel.referenced_key,
        _ => &rel.foreign_key,
    }
}

/// Embed related resources into each row of a parent array.
pub(super) async fn embed_into_rows<A: DatabaseAdapter>(
    ctx: &EmbedCtx<'_, A>,
    rel: &Relationship,
    output_name: &str,
    sub_field_names: &[String],
    embedded_filter: Option<&serde_json::Value>,
    rows: &mut [serde_json::Value],
) -> Result<(), RestError> {
    for row in rows.iter_mut() {
        embed_into_single(ctx, rel, output_name, sub_field_names, embedded_filter, row).await?;
    }
    Ok(())
}

/// Embed related resources into a single parent row.
pub(super) async fn embed_into_single<A: DatabaseAdapter>(
    ctx: &EmbedCtx<'_, A>,
    rel: &Relationship,
    output_name: &str,
    sub_field_names: &[String],
    embedded_filter: Option<&serde_json::Value>,
    row: &mut serde_json::Value,
) -> Result<(), RestError> {
    let parent_key_value = extract_join_key(ctx.schema, ctx.parent_type, row, rel);

    let Some(parent_key_value) = parent_key_value else {
        // The parent row genuinely has no join key. Since #1230 the projection is
        // guaranteed to carry the key whenever an embed needs it, so this branch
        // means the value is NULL — "no related row" — and not "the client did not
        // select the column".
        set_empty_embedding(row, output_name, rel.cardinality);
        return Ok(());
    };

    // Build WHERE clause for the sub-query: fk_column = parent_key_value.
    let mut join_predicate = serde_json::Map::new();
    join_predicate.insert(
        declared_key(ctx.schema, &rel.target_type, target_join_column(rel)),
        serde_json::json!({ "eq": parent_key_value }),
    );

    // #863 / #1170: the parent scoping and the client filter travel in *separate
    // slots* — the predicate on `QueryMatch::scope_where`, the filter in
    // `arguments["where"]` — and are AND-ed by the runner.
    //
    // #863 was the two sharing one map: the old code seeded it with the join
    // predicate and then did `where_obj.insert(k, v)` per client key, and
    // `serde_json::Map::insert` **replaces**, so a filter naming the join column
    // (`?author.id[gt]=0`, and `id` is the conventional `referenced_key` for
    // ManyToOne/OneToOne) silently destroyed the parent scoping and returned
    // another parent's children under this parent's key.
    //
    // #1170 was the two sharing one *fate*: composed into `arguments["where"]`,
    // even as an `_and` sibling, the predicate was dropped whenever the target
    // query declared `auto_params.has_where = false` — because that flag gates
    // the client filter argument, and the predicate was riding in it. Separate
    // slots fix both: they can never collide, and only the client's half is
    // gated by the client's flag.
    let client_filter = embedded_filter
        .and_then(|f| f.as_object())
        .filter(|m| !m.is_empty())
        .map(|m| serde_json::Value::Object(m.clone()));

    // Find the target type's list query.
    let target_query = find_list_query_for_type(ctx.schema, &rel.target_type);

    let Some(target_query) = target_query else {
        // No list query available — set empty default.
        set_empty_embedding(row, output_name, rel.cardinality);
        return Ok(());
    };

    let target_type_def = ctx.schema.find_type(&rel.target_type);

    // Build arguments for the sub-query.
    let mut arguments: HashMap<String, serde_json::Value> = HashMap::new();
    if let Some(filter) = client_filter {
        arguments.insert("where".to_string(), filter);
    }
    arguments.insert("limit".to_string(), serde_json::json!(ctx.config.max_page_size));

    // Build QueryMatch for the sub-query.
    let query_match = QueryMatch::from_operation(
        target_query.clone(),
        sub_field_names.to_vec(),
        arguments,
        target_type_def,
    )
    .map_err(|e| RestError::internal(format!("Failed to build embedded query: {e}")))?
    .with_scope_where(serde_json::Value::Object(join_predicate));

    let variables = serde_json::json!({});
    let vars_ref = Some(&variables);

    let result = ctx
        .executor
        .execute_query_direct(&query_match, vars_ref, ctx.security_context)
        .await
        .map_err(RestError::from)?;

    // Extract embedded data directly from the executor result.
    let embedded_data = extract_query_data(&result, &target_query.name);

    // Set the embedded data on the parent row.
    if let Some(obj) = row.as_object_mut() {
        match rel.cardinality {
            Cardinality::OneToMany => {
                // Array — empty collection returns [].
                let arr = match embedded_data {
                    Some(serde_json::Value::Array(a)) => serde_json::Value::Array(a),
                    Some(other) => serde_json::json!([other]),
                    None => serde_json::json!([]),
                };
                obj.insert(output_name.to_string(), arr);
            },
            Cardinality::ManyToOne | Cardinality::OneToOne => {
                // Single object or null.
                let val = match embedded_data {
                    Some(serde_json::Value::Array(mut a)) if !a.is_empty() => a.remove(0),
                    Some(other) => other,
                    None => serde_json::Value::Null,
                };
                obj.insert(output_name.to_string(), val);
            },
            _ => {
                obj.insert(
                    output_name.to_string(),
                    embedded_data.unwrap_or(serde_json::Value::Null),
                );
            },
        }
    }

    Ok(())
}

/// Extract the join key value from a parent row.
///
/// Reads [`parent_join_column`] in the spelling `parent_type` publishes it under,
/// because the row was projected under declared names, not storage ones — see
/// [`declared_key`].
///
/// `None` means the key is absent or NULL. Both now mean the same thing: no related
/// row. They did not before #1230, when a projection that simply omitted the column
/// landed in this branch too, and every `ManyToOne` embed the client had not
/// hand-selected the foreign key for came back null under a 200.
pub(super) fn extract_join_key(
    schema: &CompiledSchema,
    parent_type: &str,
    row: &serde_json::Value,
    rel: &Relationship,
) -> Option<serde_json::Value> {
    let key_field = declared_key(schema, parent_type, parent_join_column(rel));
    row.get(key_field.as_str()).cloned().filter(|v| !v.is_null())
}

/// Set the appropriate empty default for an embedding.
pub(super) fn set_empty_embedding(
    row: &mut serde_json::Value,
    output_name: &str,
    cardinality: Cardinality,
) {
    if let Some(obj) = row.as_object_mut() {
        match cardinality {
            Cardinality::OneToMany => {
                obj.insert(output_name.to_string(), serde_json::json!([]));
            },
            _ => {
                obj.insert(output_name.to_string(), serde_json::Value::Null);
            },
        }
    }
}

/// Find a list query that returns the given type.
pub(super) fn find_list_query_for_type<'a>(
    schema: &'a CompiledSchema,
    type_name: &str,
) -> Option<&'a fraiseql_core::schema::QueryDefinition> {
    schema.queries.iter().find(|q| q.return_type == type_name && q.returns_list)
}

/// Extract data from executor query result envelope.
pub(super) fn extract_query_data(
    parsed: &serde_json::Value,
    query_name: &str,
) -> Option<serde_json::Value> {
    parsed.get("data").and_then(|d| d.get(query_name)).cloned()
}

/// Count related resources for a single parent row.
pub(super) async fn count_related<A: DatabaseAdapter>(
    ctx: &EmbedCtx<'_, A>,
    rel: &Relationship,
    row: &serde_json::Value,
) -> Result<u64, RestError> {
    let parent_key_value = extract_join_key(ctx.schema, ctx.parent_type, row, rel);

    let Some(parent_key_value) = parent_key_value else {
        return Ok(0);
    };

    let mut join_predicate = serde_json::Map::new();
    join_predicate.insert(
        declared_key(ctx.schema, &rel.target_type, target_join_column(rel)),
        serde_json::json!({ "eq": parent_key_value }),
    );

    let target_query = find_list_query_for_type(ctx.schema, &rel.target_type);
    let Some(target_query) = target_query else {
        return Ok(0);
    };

    let target_type_def = ctx.schema.find_type(&rel.target_type);

    // #1170: the scoping slot, not `arguments["where"]` — `count_rows` gates that
    // argument on the target's `has_where` exactly as the read path does, so a
    // predicate riding in it produced the whole table's count under a parent's key.
    let arguments: HashMap<String, serde_json::Value> = HashMap::new();

    let query_match =
        QueryMatch::from_operation(target_query.clone(), Vec::new(), arguments, target_type_def)
            .map_err(|e| RestError::internal(format!("Failed to build count query: {e}")))?
            .with_scope_where(serde_json::Value::Object(join_predicate));

    let variables = serde_json::json!({});
    let vars_ref = Some(&variables);

    let count = ctx
        .executor
        .count_rows(&query_match, vars_ref, ctx.security_context)
        .await
        .map_err(RestError::from)?;

    Ok(count)
}
