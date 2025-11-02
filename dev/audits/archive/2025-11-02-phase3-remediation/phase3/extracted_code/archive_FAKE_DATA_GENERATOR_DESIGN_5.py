# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 5
class FakeDataGenerator:
    """Generate fake data for FraiseQL trinity pattern"""

    def __init__(
        self, db_connection, scenario_id: int, locale: str = "en_US", seed: int | None = None
    ):
        self.db = db_connection
        self.introspector = SchemaIntrospector(db_connection)
        self.uuid_gen = SemanticUUIDGenerator(scenario_id)
        self.faker_provider = FakerProvider(locale, seed)

        # NO UUID MAPPING NEEDED! ✨
        # FKs use integers directly from pk_* columns

    def insert_generated_data(self, table: str, rows: list[dict[str, Any]]) -> list[int]:
        """Insert rows and return generated integer PKs"""
        metadata = self.introspector.get_table_metadata(table)
        pk_col = metadata.pk_column  # "pk_language"

        pks = []

        for row in rows:
            cols = ", ".join(row.keys())
            placeholders = ", ".join(["%s"] * len(row))

            query = f"""
                INSERT INTO {table} ({cols})
                VALUES ({placeholders})
                RETURNING {pk_col}
            """

            result = self.db.execute(query, list(row.values())).fetchone()
            pk_int = result[0]
            pks.append(pk_int)

        return pks  # Return integers for child FK references
