# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 6
# ❌ OLD: Direct list/dict returns
result = await repo.find("users")
assert isinstance(result, list)

# RustResponseBytes wrapper
result = await repo.find("users")
assert isinstance(result, RustResponseBytes)

# Extract data for testing:
from tests.unit.utils.test_response_utils import extract_graphql_data

users = extract_graphql_data(result, "users")
assert isinstance(users, list)
