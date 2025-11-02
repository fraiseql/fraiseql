# Extracted from: docs/core/database-api.md
# Block number: 1
# Exclusive Rust pipeline methods:
users = await repo.find_rust("v_user", "users", info)
user = await repo.find_one_rust("v_user", "user", info, id=123)
filtered = await repo.find_rust("v_user", "users", info, age__gt=18)
