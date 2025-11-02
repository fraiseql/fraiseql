# Extracted from: docs/architecture/DIRECT_PATH_IMPLEMENTATION.md
# Block number: 2
# 1. Parse GraphQL query
parsed = parse_graphql_query_simple(request.query)

# 2. Determine entity and view
entity_name = field_name.rstrip("s")  # "users" → "user"
view_name = f"v_{entity_name}"  # → "v_user"

# 3. Build SQL query (WHERE/LIMIT/ORDER BY)
query = db._build_find_query(
    view_name=view_name,
    field_paths=None,  # Rust does projection
    jsonb_column="data",
    **arguments,
)

# 4. Execute via Rust pipeline
result_bytes = await execute_via_rust_pipeline(
    conn=conn,
    query=query.statement,
    params=query.params,
    field_name=field_name,
    type_name=type_name,
    is_list=is_list,
    field_paths=field_paths,
)

# 5. Return bytes directly to HTTP
return Response(content=bytes(result_bytes), media_type="application/json")
