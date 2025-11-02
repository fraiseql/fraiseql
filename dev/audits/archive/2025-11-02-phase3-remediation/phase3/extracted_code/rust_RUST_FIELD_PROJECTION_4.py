# Extracted from: docs/rust/RUST_FIELD_PROJECTION.md
# Block number: 4
# fraiseql/config.py

# SECURITY: Field projection is MANDATORY and ALWAYS enabled
# There is no "disable" option - this is a security requirement

# Optional: Enable debug logging to see which fields are filtered
FIELD_PROJECTION_LOG_FILTERED = False  # Set to True for debugging

# Example log output when enabled:
# DEBUG: Projected fields for users query: ["id", "first_name", "email"]
# DEBUG: Filtered out 17 fields: ["ssn", "password_hash", "internal_notes", ...]
