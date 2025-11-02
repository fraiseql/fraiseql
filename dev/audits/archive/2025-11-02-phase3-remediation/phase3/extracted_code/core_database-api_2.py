# Extracted from: docs/core/database-api.md
# Block number: 2
# Direct database access (bypasses Rust pipeline)
users = await repo.find("v_user")
user = await repo.find_one("v_user", id=123)
