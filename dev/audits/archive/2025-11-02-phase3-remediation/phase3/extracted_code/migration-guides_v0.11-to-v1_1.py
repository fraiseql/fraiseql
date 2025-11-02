# Extracted from: docs/migration-guides/v0.11-to-v1.md
# Block number: 1
# Before (v0.11)
result = await repository.find_raw_json("users", "data")
user = await repository.find_one_raw_json("users", "data", id=123)

# After (v1)
result = await repository.find("users")
user = await repository.find_one("users", id=123)
