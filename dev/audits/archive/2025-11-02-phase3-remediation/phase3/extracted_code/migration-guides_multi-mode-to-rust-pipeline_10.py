# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 10
result = await repo.find("users")
users = extract_graphql_data(result, "users")
assert len(users) > 0  # ✅ Now works
