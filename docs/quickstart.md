# Quick Start Guide

Get started with FraiseQL in minutes!

## Installation

```bash
pip install fraiseql[all]
```

## Basic Setup

### 1. Create a Simple Schema

```python
# app.py
import fraiseql
from fraiseql import Info
from typing import List

@fraiseql.type
class User:
    id: int
    name: str
    email: str

@fraiseql.query
def get_users(info: Info) -> List[User]:
    return info.context.repo.find("users_view")
```

### 2. Set Up Database

```python
from fraiseql.db import FraiseQLRepository
import asyncpg

# Create connection pool
pool = await asyncpg.create_pool(
    "postgresql://user:password@localhost/mydb"
)

# Create repository
repo = FraiseQLRepository(pool)
```

### 3. Create FastAPI App

```python
from fastapi import FastAPI
from fraiseql.fastapi import FraiseQLRouter

app = FastAPI()

router = FraiseQLRouter(
    repo=repo,
    schema=fraiseql.build_schema()
)

app.include_router(router, prefix="/graphql")
```

### 4. Run Your Server

```bash
uvicorn app:app --reload
```

Visit `http://localhost:8000/graphql` to see the GraphQL playground!

## Your First Query

Try this query in the playground:

```graphql
query {
  users {
    id
    name
    email
  }
}
```

## Next Steps

- [Full Documentation](../README.md)
- [Advanced Features](advanced/)
- [Performance Guide](../PERFORMANCE_GUIDE.md)
- [Examples](../examples/)

## Need Help?

- [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)
- [Documentation](https://docs.fraiseql.com)
- [Examples](../examples/)

---

Ready to build something amazing? Let's go! 🚀
