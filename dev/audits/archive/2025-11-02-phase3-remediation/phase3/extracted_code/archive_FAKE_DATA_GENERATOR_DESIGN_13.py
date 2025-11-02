# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 13
def insert_generated_data(self, table, rows):
    for row in rows:
        pk_int = db.insert(...).returning("pk_language")
        # No mapping needed!

    return pks


def _resolve_fk_integer(self, col_meta):
    # Direct query - no mapping
    pk = db.execute(f"SELECT {col_meta.fk_column} FROM {col_meta.fk_table} LIMIT 1")
    return pk  # Return integer for FK
