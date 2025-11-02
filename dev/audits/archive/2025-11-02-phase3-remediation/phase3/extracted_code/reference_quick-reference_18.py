# Extracted from: docs/reference/quick-reference.md
# Block number: 18
import asyncpg
from fastapi import FastAPI

from fraiseql.db import FraiseQLRepository
from fraiseql.fastapi import FraiseQLRouter

# Database connection
pool = await asyncpg.create_pool("postgresql://user:pass@localhost/mydb")
repo = FraiseQLRepository(pool)

# FastAPI app
app = FastAPI()
router = FraiseQLRouter(repo=repo, schema=fraiseql.build_schema())
app.include_router(router, prefix="/graphql")
