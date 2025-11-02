# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 3
class SemanticUUIDGenerator:
    """Generate deterministic, metadata-encoded UUIDs"""

    def generate(self, table_code: int, sequence: int | None = None) -> uuid.UUID:
        """Generate UUID for table"""
        if sequence is None:
            sequence = self._next_sequence(table_code)

        # Encode: table_code (32) | scenario (16) | version (16) | sequence (64)
        uuid_bytes = (
            table_code.to_bytes(4, "big")
            + self.scenario_id.to_bytes(2, "big")
            + self.version.to_bytes(2, "big")
            + sequence.to_bytes(8, "big")
        )
        return uuid.UUID(bytes=uuid_bytes)
