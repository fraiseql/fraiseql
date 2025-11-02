# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 4
@dataclass
class ColumnMetadata:
    name: str
    type: str
    is_pk: bool  # True for pk_entity
    is_uuid_id: bool  # True for 'id' column (NEW)
    is_fk: bool
    fk_table: str | None
    fk_column: str | None  # e.g., "pk_continent" (INTEGER!)
    is_nullable: bool
    is_identifier: bool
    is_audit: bool


@dataclass
class TableMetadata:
    schema: str
    name: str
    table_code: int
    columns: dict[str, ColumnMetadata]

    pk_column: str  # "pk_language" (INTEGER)
    uuid_id_column: str  # "id" (UUID) - NEW!
    identifier_column: str | None  # "identifier" (slug)
