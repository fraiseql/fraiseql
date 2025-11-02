# Extracted from: docs/rust/RUST_FIRST_PIPELINE.md
# Block number: 8
from fraiseql.core.rust_pipeline import (
    RustResponseBytes,
    execute_via_rust_pipeline,
)


class FraiseQLRepository(PassthroughMixin):
    async def find_rust(
        self, view_name: str, field_name: str, info: Any = None, **kwargs
    ) -> RustResponseBytes:
        """Find records using Rust-first pipeline.

        This is the FASTEST method - uses PostgreSQL → Rust → HTTP path
        with ZERO Python string operations.

        Returns RustResponseBytes that FastAPI sends directly as HTTP.
        """
        # Extract field paths from GraphQL info
        field_paths = None
        if info:
            from fraiseql.core.ast_parser import extract_field_paths_from_info
            from fraiseql.utils.casing import to_snake_case

            field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

        # Get cached JSONB column (no sample query!)
        jsonb_column = None
        if view_name in _table_metadata:
            jsonb_column = _table_metadata[view_name].get("jsonb_column", "data")
        else:
            jsonb_column = "data"  # Default

        # Build query
        query = self._build_find_query(
            view_name,
            raw_json=True,
            field_paths=field_paths,
            info=info,
            jsonb_column=jsonb_column,
            **kwargs,
        )

        # Get cached type name
        type_name = self._get_cached_type_name(view_name)

        # 🚀 EXECUTE VIA RUST PIPELINE
        async with self._pool.connection() as conn:
            return await execute_via_rust_pipeline(
                conn,
                query.statement,
                query.params,
                field_name,
                type_name,
                is_list=True,
            )

    async def find_one_rust(
        self, view_name: str, field_name: str, info: Any = None, **kwargs
    ) -> RustResponseBytes:
        """Find single record using Rust-first pipeline."""
        # Similar to find_rust but is_list=False
        # ... (implementation similar to above)

        async with self._pool.connection() as conn:
            return await execute_via_rust_pipeline(
                conn,
                query.statement,
                query.params,
                field_name,
                type_name,
                is_list=False,
            )
