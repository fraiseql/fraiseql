# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 12
def insert_generated_data(self, table, rows):
    for row in rows:
        pk_int = db.insert(...).returning("id")

        # Extract UUID from row to store mapping
        uuid_col = metadata.uuid_pk_column  # "pk_language"
        if uuid_col in row:
            self._uuid_to_pk[row[uuid_col]] = pk_int  # Track mapping

    return pks


def _resolve_foreign_key(self, col_meta):
    # Need to maintain/lookup UUID mapping
    parent_uuid = self._get_parent_uuid(...)
    return parent_uuid  # Return UUID for FK
