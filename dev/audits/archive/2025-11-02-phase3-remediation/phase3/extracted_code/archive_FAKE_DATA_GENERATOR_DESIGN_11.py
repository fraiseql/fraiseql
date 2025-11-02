# Extracted from: docs/archive/FAKE_DATA_GENERATOR_DESIGN.md
# Block number: 11
# Optional: Cache parent PKs for round-robin (only parent tables)
self._parent_pk_cache: dict[str, list[int]] = {}
# For 7 continents → 1 list with 7 integers → ~100 bytes
