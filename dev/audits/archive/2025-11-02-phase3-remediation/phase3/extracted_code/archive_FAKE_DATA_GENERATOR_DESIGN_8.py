# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 8
def _resolve_fk_integer(self, col_meta: ColumnMetadata) -> int:
    """Pick random parent row"""
    result = self.db.execute(f"""
        SELECT {col_meta.fk_column}
        FROM {col_meta.fk_table}
        WHERE deleted_at IS NULL
        ORDER BY random()
        LIMIT 1
    """).fetchone()
    return result[0]
