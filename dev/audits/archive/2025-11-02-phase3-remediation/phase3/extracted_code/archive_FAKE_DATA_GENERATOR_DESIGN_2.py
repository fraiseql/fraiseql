# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 2
def _resolve_fk_integer(self, col_meta: ColumnMetadata) -> int:
    """Resolve FK by getting integer PK from parent table"""
    fk_table = col_meta.fk_table
    pk_col = col_meta.fk_column  # e.g., "pk_continent"

    # Simple query - no UUID mapping needed!
    result = self.db.execute(f"""
        SELECT {pk_col}
        FROM {fk_table}
        WHERE deleted_at IS NULL
        ORDER BY random()  -- Or LIMIT 1 for deterministic
        LIMIT 1
    """).fetchone()

    if not result:
        raise ValueError(f"No data in {fk_table} for FK {col_meta.name}")

    return result[0]  # Return integer directly
