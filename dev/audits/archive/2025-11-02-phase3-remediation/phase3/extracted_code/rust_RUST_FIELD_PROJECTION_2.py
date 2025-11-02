# Extracted from: docs/rust/RUST_FIELD_PROJECTION.md
# Block number: 2
# src/fraiseql/db.py


async def find_rust(self, view_name: str, field_name: str, info: Any, **kwargs):
    #                                                           ↑
    #                                             NO LONGER Any | None
    #                                             info is REQUIRED for security

    # Extract field paths from GraphQL info (REQUIRED for security)
    from fraiseql.core.ast_parser import extract_field_paths_from_info
    from fraiseql.utils.casing import to_snake_case

    # Get list of requested fields
    field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

    # Convert FieldPath objects to simple list of field names
    field_selection = [path.field if hasattr(path, "field") else str(path) for path in field_paths]

    if not field_selection:
        raise ValueError(
            f"Field selection is empty for {view_name}. "
            "This is a security requirement - GraphQL info must provide field selection."
        )

    logger.debug(f"Field selection for {view_name}: {field_selection}")

    # Pass to Rust pipeline (field_selection is REQUIRED parameter)
    async with self._pool.connection() as conn:
        return await execute_via_rust_pipeline(
            conn,
            query.statement,
            query.params,
            field_name,
            type_name,
            is_list=True,
            field_selection=field_selection,  # ← REQUIRED (not optional)
        )
