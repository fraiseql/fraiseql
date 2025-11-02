# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 10
# Need to track UUID→Integer mapping for ALL entities
self._uuid_to_pk: dict[uuid.UUID, int] = {}
# For 100K rows → 100K dict entries → ~3-4MB memory
