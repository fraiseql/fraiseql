# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 12
from tests.unit.utils.test_response_utils import extract_graphql_data

from fraiseql import FraiseQLConfig
from fraiseql.db import FraiseQLRepository

config = FraiseQLConfig(database_url="postgresql://...")

repo = FraiseQLRepository(pool)
result = await repo.find("users", where={"status": {"eq": "active"}})

# Result is RustResponseBytes - extract for processing
users = extract_graphql_data(result, "users")

# Use GraphQL field names
for user in users:
    print(f"{user['firstName']} - {user['email']}")
