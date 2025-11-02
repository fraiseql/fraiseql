# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 9
# Database: first_name, is_active
# GraphQL: firstName, isActive

user = users[0]
assert user["firstName"] == "John"  # ✅ Correct
assert user["first_name"] == "John"  # ❌ Wrong
