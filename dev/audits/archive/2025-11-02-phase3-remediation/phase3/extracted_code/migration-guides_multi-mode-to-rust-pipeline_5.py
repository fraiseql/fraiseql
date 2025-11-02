# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 5
# ❌ OLD: Python field names
assert user.first_name == "John"
assert user.is_active is True

# GraphQL camelCase field names
assert user["firstName"] == "John"
assert user["isActive"] is True
