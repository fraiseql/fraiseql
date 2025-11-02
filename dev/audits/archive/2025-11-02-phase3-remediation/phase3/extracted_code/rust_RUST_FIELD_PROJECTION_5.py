# Extracted from: docs/rust/RUST_FIELD_PROJECTION.md
# Block number: 5
# Development/debugging mode - see what's being filtered
FIELD_PROJECTION_LOG_FILTERED = True
FIELD_PROJECTION_LOG_LEVEL = "DEBUG"

# Example detailed log output:
# DEBUG: Field projection for users (query_id=abc123):
#   - Requested: ["id", "first_name", "email"] (3 fields)
#   - Available in JSONB: 20 fields
#   - Filtered out: ["ssn", "password_hash", "internal_notes", ...] (17 fields)
#   - Bandwidth saved: 1.8KB per row (90%)
