# Extracted from: docs/api-reference/database.md
# Block number: 1
import asyncpg

from fraiseql.db import FraiseQLRepository

pool = await asyncpg.create_pool("postgresql://...")
repo = FraiseQLRepository(pool, context=None)
