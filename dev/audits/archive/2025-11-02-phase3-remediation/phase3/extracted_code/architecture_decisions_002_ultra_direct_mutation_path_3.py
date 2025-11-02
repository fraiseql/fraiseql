# Extracted from: docs/architecture/decisions/002_ultra_direct_mutation_path.md
# Block number: 3
# src/fraiseql/db.py


async def execute_function_raw_json(
    self,
    function_name: str,
    input_data: dict[str, object],
    type_name: str | None = None,
) -> RawJSONResult:
    """Execute a PostgreSQL function and return raw JSON (no parsing).

    This is the ultra-direct path for mutations:
    PostgreSQL JSONB::text → Rust transform → RawJSONResult → Client

    Args:
        function_name: Fully qualified function name (e.g., 'app.delete_customer')
        input_data: Dictionary to pass as JSONB to the function
        type_name: GraphQL type name for Rust __typename injection

    Returns:
        RawJSONResult with transformed JSON (camelCase + __typename)
    """
    import json

    # Validate function name to prevent SQL injection
    if not function_name.replace("_", "").replace(".", "").isalnum():
        msg = f"Invalid function name: {function_name}"
        raise ValueError(msg)

    async with self._pool.connection() as conn, conn.cursor() as cursor:
        # Set session variables from context
        await self._set_session_variables(cursor)

        # Execute function and get JSONB as text (no Python parsing!)
        # The ::text cast ensures we get a string, not a parsed dict
        await cursor.execute(
            f"SELECT {function_name}(%s::jsonb)::text",
            (json.dumps(input_data),),
        )
        result = await cursor.fetchone()

        if not result or result[0] is None:
            # Return error response as raw JSON
            error_json = json.dumps(
                {"success": False, "code": "INTERNAL_ERROR", "message": "Function returned null"}
            )
            return RawJSONResult(error_json, transformed=False)

        # Get the raw JSON string (no parsing!)
        json_string = result[0]

        # Apply Rust transformation if type provided
        if type_name:
            logger.debug(f"🦀 Transforming mutation result with Rust (type: {type_name})")

            # Use Rust transformer (same as queries!)
            from fraiseql.core.rust_transformer import get_transformer

            transformer = get_transformer()

            try:
                # Register type if needed
                # (Type should already be registered, but ensure it)
                # Rust will inject __typename and convert to camelCase
                transformed_json = transformer.transform(json_string, type_name)

                logger.debug("✅ Rust transformation completed")
                return RawJSONResult(transformed_json, transformed=True)

            except Exception as e:
                logger.warning(f"⚠️  Rust transformation failed: {e}, returning original JSON")
                return RawJSONResult(json_string, transformed=False)

        # No type provided, return as-is (no transformation)
        return RawJSONResult(json_string, transformed=False)
