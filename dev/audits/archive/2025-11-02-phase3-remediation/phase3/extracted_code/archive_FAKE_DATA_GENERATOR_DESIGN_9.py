# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 9
def _resolve_fk_integer(self, col_meta: ColumnMetadata) -> int:
    """Distribute children evenly across parents"""
    # Cache parent PKs
    cache_key = col_meta.fk_table
    if cache_key not in self._parent_pk_cache:
        results = self.db.execute(f"""
            SELECT {col_meta.fk_column}
            FROM {col_meta.fk_table}
            WHERE deleted_at IS NULL
            ORDER BY {col_meta.fk_column}
        """).fetchall()
        self._parent_pk_cache[cache_key] = [r[0] for r in results]

    parent_pks = self._parent_pk_cache[cache_key]

    # Round-robin selection
    idx = self._fk_counter.get(cache_key, 0) % len(parent_pks)
    self._fk_counter[cache_key] = idx + 1

    return parent_pks[idx]
