# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 11
from fraiseql import ExecutionMode, FraiseQLConfig
from fraiseql.db import FraiseQLRepository

config = FraiseQLConfig(database_url="postgresql://...", execution_mode=ExecutionMode.TURBO)

repo = FraiseQLRepository(pool, context={"mode": "turbo"})
result = await repo.find("users", where={"status": {"eq": "active"}})

# Result was Python list
for user in result:
    print(f"{user.first_name} - {user.email}")
