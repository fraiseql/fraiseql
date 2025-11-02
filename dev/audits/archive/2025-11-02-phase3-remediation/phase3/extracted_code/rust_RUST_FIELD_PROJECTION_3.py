# Extracted from: docs/rust/RUST_FIELD_PROJECTION.md
# Block number: 3
# src/fraiseql/core/rust_pipeline.py


async def execute_via_rust_pipeline(
    conn: AsyncConnection,
    query: Composed | SQL,
    params: dict[str, Any] | None,
    field_name: str,
    type_name: str | None,
    field_selection: list[str],  # ← REQUIRED parameter (not Optional)
    is_list: bool = True,
) -> RustResponseBytes:
    """Execute query and build HTTP response with MANDATORY field projection in Rust.

    SECURITY: field_selection is REQUIRED. Never send unrequested fields to clients.

    Args:
        conn: PostgreSQL connection
        query: SQL query returning JSON strings
        params: Query parameters
        field_name: GraphQL field name for wrapping
        type_name: GraphQL type for transformation (optional)
        field_selection: List of field names to include (snake_case) - REQUIRED
                        Example: ["id", "first_name", "email"]
                        This is a SECURITY REQUIREMENT, not optional.
        is_list: True for arrays, False for single objects

    Raises:
        ValueError: If field_selection is empty (security violation)
    """
    if not field_selection:
        raise ValueError(
            "field_selection is required for security. "
            "Cannot send unfiltered JSONB data to clients."
        )

    async with conn.cursor() as cursor:
        await cursor.execute(query, params or {})

        if is_list:
            rows = await cursor.fetchall()
            json_strings = [row[0] for row in rows if row[0] is not None]

            # 🔒 Rust ALWAYS filters to field_selection (security requirement)
            response_bytes = fraiseql_rs.build_list_response(
                json_strings,
                field_name,
                type_name,
                field_selection,  # ← REQUIRED: Rust always filters
            )

            return RustResponseBytes(response_bytes)
        row = await cursor.fetchone()

        if not row or row[0] is None:
            response_bytes = fraiseql_rs.build_null_response(field_name)
            return RustResponseBytes(response_bytes)

        json_string = row[0]

        # 🔒 Rust ALWAYS filters to field_selection (security requirement)
        response_bytes = fraiseql_rs.build_single_response(
            json_string,
            field_name,
            type_name,
            field_selection,  # ← REQUIRED: Rust always filters
        )

        return RustResponseBytes(response_bytes)
