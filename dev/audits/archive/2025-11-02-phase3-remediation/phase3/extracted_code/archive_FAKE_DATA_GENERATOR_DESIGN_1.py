# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 1
def _generate_single_row(
    self, metadata: TableMetadata, overrides: dict[str, Any] | None
) -> dict[str, Any]:
    """Generate single row - SIMPLIFIED for FraiseQL pattern"""
    row = {}

    # 1. Generate stable UUID for 'id' field (not pk_entity!)
    row["id"] = self.uuid_gen.generate(metadata.table_code)

    # 2. Skip pk_entity - DB auto-generates
    # 3. Skip fk_* initially - resolve after parent insertion

    for col_name, col_meta in metadata.columns.items():
        if col_name == "id":  # Already set
            continue
        if col_name == metadata.pk_column:  # pk_entity - DB handles
            continue

        if col_meta.is_fk:
            # FKs are integers - resolve from parent table
            row[col_name] = self._resolve_fk_integer(col_meta)
        elif col_meta.is_identifier:
            row[col_name] = self._generate_identifier(metadata.name)
        elif col_meta.is_audit:
            row[col_name] = self._generate_audit_value(col_name)
        else:
            row[col_name] = self._generate_fake_value(metadata.name, col_name, col_meta.type)

    return row
