"""SQL helper utilities for FraiseQL PostgreSQL functions."""


def generate_partial_update_checks(fields: dict[str, str]) -> str:
    """Generate SQL CASE statements for partial updates.

    Args:
        fields: Dict mapping GraphQL field names to PostgreSQL column names

    Returns:
        SQL fragment for UPDATE SET clause

    Example:
        >>> fields = {"ipAddress": "ip_address", "hostname": "hostname"}
        >>> generate_partial_update_checks(fields)
        'ip_address = CASE WHEN p_input ? 'ipAddress' THEN ' + \
         "p_input->>'ipAddress' ELSE ip_address END,
         hostname = CASE WHEN p_input ? 'hostname' THEN p_input->>'hostname' ELSE hostname END'
    """
    updates = []
    for graphql_name, pg_column in fields.items():
        updates.append(
            f"{pg_column} = CASE "
            f"WHEN p_input ? '{graphql_name}' THEN p_input->>'{graphql_name}' "
            f"ELSE {pg_column} END"
        )
    return ",\n        ".join(updates)


def generate_field_update_blocks(
    fields: dict[str, str], table_name: str, schema: str = "public"
) -> str:
    """Generate individual IF blocks for field updates.

    Args:
        fields: Dict mapping GraphQL field names to PostgreSQL column names
        table_name: Name of the table to update
        schema: Schema name (default: public)

    Returns:
        SQL fragment with IF blocks for each field
    """
    blocks = []
    for graphql_name, pg_column in fields.items():
        block = f"""    IF p_input ? '{graphql_name}' THEN
        UPDATE {schema}.{table_name}
        SET {pg_column} = p_input->>'{graphql_name}',
            updated_at = CURRENT_TIMESTAMP
        WHERE id = v_id;
        v_updated_fields := array_append(v_updated_fields, '{graphql_name}');
    END IF;"""
        blocks.append(block)
    return "\n\n".join(blocks)


def generate_jsonb_build_object(fields: dict[str, str], table_alias: str = "") -> str:
    """Generate jsonb_build_object call for SELECT INTO.

    Args:
        fields: Dict mapping GraphQL field names to PostgreSQL column names
        table_alias: Optional table alias prefix

    Returns:
        SQL fragment for jsonb_build_object
    """
    prefix = f"{table_alias}." if table_alias else ""
    pairs = []
    for graphql_name, pg_column in fields.items():
        pairs.append(f"'{graphql_name}', {prefix}{pg_column}")
    return "jsonb_build_object(\n            " + ",\n            ".join(pairs) + "\n        )"


def check_field_exists(jsonb_expr: str, field_name: str, check_camel_case: bool = True) -> str:
    """Generate SQL expression to check if field exists in JSONB.

    Args:
        jsonb_expr: JSONB expression (e.g., 'p_input')
        field_name: Field name to check
        check_camel_case: Whether to also check camelCase variant

    Returns:
        SQL boolean expression
    """
    if not check_camel_case:
        return f"{jsonb_expr} ? '{field_name}'"

    # Convert snake_case to camelCase
    parts = field_name.split("_")
    camel_case = parts[0] + "".join(word.capitalize() for word in parts[1:])

    if camel_case == field_name:
        return f"{jsonb_expr} ? '{field_name}'"

    return f"({jsonb_expr} ? '{field_name}' OR {jsonb_expr} ? '{camel_case}')"


def get_jsonb_field_value(jsonb_expr: str, field_name: str, check_camel_case: bool = True) -> str:
    """Generate SQL expression to get field value from JSONB.

    Args:
        jsonb_expr: JSONB expression (e.g., 'p_input')
        field_name: Field name to get
        check_camel_case: Whether to also check camelCase variant

    Returns:
        SQL expression to get the value
    """
    if not check_camel_case:
        return f"{jsonb_expr}->>'{field_name}'"

    # Convert snake_case to camelCase
    parts = field_name.split("_")
    camel_case = parts[0] + "".join(word.capitalize() for word in parts[1:])

    if camel_case == field_name:
        return f"{jsonb_expr}->>'{field_name}'"

    return f"COALESCE({jsonb_expr}->>'{field_name}', {jsonb_expr}->>'{camel_case}')"


# SQL template for partial update function
PARTIAL_UPDATE_FUNCTION_TEMPLATE = """
CREATE OR REPLACE FUNCTION {schema}.{function_name}(
    p_input JSONB
) RETURNS {schema}.mutation_result AS $$
DECLARE
    v_result {schema}.mutation_result;
    v_id UUID;
    v_updated_fields TEXT[] := '{{}}';
    v_update_count INT := 0;
BEGIN
    -- Validate input
    IF NOT p_input ? 'id' THEN
        v_result.status := 'error';
        v_result.message := '{entity} ID is required';
        RETURN v_result;
    END IF;

    v_id := (p_input->>'id')::UUID;

    -- Check if {entity} exists
    IF NOT EXISTS (SELECT 1 FROM {schema}.{table_name} WHERE id = v_id) THEN
        v_result.status := 'error';
        v_result.message := '{entity} not found';
        RETURN v_result;
    END IF;

    -- Update only provided fields
{field_updates}

    -- Build result with full object data
    SELECT INTO v_result.object_data
        {jsonb_object}
    FROM {schema}.{table_name}
    WHERE id = v_id;

    v_result.id := v_id;
    v_result.status := 'success';
    v_result.message := format('{entity} updated successfully (%s fields)', v_update_count);
    v_result.updated_fields := v_updated_fields;
    v_result.extra_metadata := jsonb_build_object(
        'entity', '{entity_lower}',
        'operation', 'update',
        'fields_updated', v_update_count
    );

    RETURN v_result;
EXCEPTION
    WHEN OTHERS THEN
        v_result.status := 'error';
        v_result.message := format('Update failed: %s', SQLERRM);
        RETURN v_result;
END;
$$ LANGUAGE plpgsql;
"""


def generate_partial_update_function(
    function_name: str,
    table_name: str,
    entity_name: str,
    fields: dict[str, str],
    schema: str = "app",
    include_timestamps: bool = True,
) -> str:
    """Generate a complete partial update function.

    Args:
        function_name: Name of the PostgreSQL function
        table_name: Name of the table to update
        entity_name: Human-readable entity name (e.g., "Router")
        fields: Dict mapping GraphQL field names to PostgreSQL column names
        schema: Schema name (default: app)
        include_timestamps: Whether to include created_at/updated_at in output

    Returns:
        Complete SQL function definition
    """
    # Add timestamp fields if requested
    output_fields = fields.copy()
    if include_timestamps:
        output_fields["createdAt"] = "created_at"
        output_fields["updatedAt"] = "updated_at"

    # Generate field update blocks
    field_updates = generate_field_update_blocks(fields, table_name, schema)

    # Add update count increment to each block
    field_updates = field_updates.replace(
        "v_updated_fields := array_append(v_updated_fields,",
        "v_update_count := v_update_count + 1;\n        "
        "v_updated_fields := array_append(v_updated_fields,",
    )

    # Generate JSONB object for result
    jsonb_object = generate_jsonb_build_object(output_fields)

    return PARTIAL_UPDATE_FUNCTION_TEMPLATE.format(
        schema=schema,
        function_name=function_name,
        entity=entity_name,
        entity_lower=entity_name.lower(),
        table_name=table_name,
        field_updates=field_updates,
        jsonb_object=jsonb_object,
    )
