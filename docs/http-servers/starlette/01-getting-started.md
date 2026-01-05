# Getting Started with Starlette

**Version**: 2.0.0+
**Reading Time**: 30 minutes
**Audience**: New Starlette users, Python developers
**Difficulty**: Easy (Python knowledge sufficient)
**Prerequisites**: Python 3.13+, PostgreSQL 13+

---

## What You'll Learn

In this guide, you'll:
- ✅ Understand what Starlette brings to FraiseQL
- ✅ Set up your development environment
- ✅ Build your first Starlette-based GraphQL server
- ✅ Test it locally with GraphQL clients
- ✅ Understand how Starlette differs from Axum and FastAPI

---

## What is Starlette?

Starlette is a lightweight, high-performance ASGI web framework for Python that powers FraiseQL v2.0.0's alternative HTTP server layer.

### Why Starlette?

```
Your GraphQL Types & Resolvers (Python)
           ↓
    Python HTTP Layer
    ┌──────────────────────────────────┐
    │ Starlette Web Framework          │
    │ • ASGI-native (async/await)      │
    │ • High performance (async I/O)   │
    │ • Minimal overhead               │
    │ • WebSocket support              │
    │ • Middleware ecosystem           │
    │ • Pure Python (no Rust needed)   │
    └──────────────────────────────────┘
           ↓
Exclusive Rust GraphQL Pipeline
```

### Key Benefits

| Benefit | Why It Matters |
|---------|----------------|
| **Pure Python** | No Rust needed, Python ecosystem familiar |
| **5x faster than FastAPI** | High performance with minimal code |
| **Lightweight** | Simple codebase, easy to understand |
| **ASGI-native** | Native async/await support throughout |
| **WebSocket subscriptions** | Real-time updates (graphql-ws) |
| **Excellent middleware** | Clean middleware pattern |

### When to Use Starlette

✅ **Use Starlette if**:
- Prefer pure Python (no Rust)
- Team knows Python well
- Want lightweight alternative to FastAPI
- Need real-time features (WebSocket)
- Value code simplicity and clarity

❌ **Use Axum if**:
- Need maximum performance (7-10x faster)
- Have Rust expertise available
- Building high-frequency trading/gaming APIs
- Performance is critical requirement

✅ **Use FastAPI if**:
- Need existing FastAPI ecosystem
- Backward compatibility required
- Legacy project migration

---

## Prerequisites

Before starting, you need:

### 1. Python Installation

**Check if you have Python**:
```bash
python --version   # Should be 3.13+
python3 --version  # Alternative on some systems
```

**If not installed**:
```bash
# macOS
brew install python@3.13

# Ubuntu/Debian
sudo apt-get install python3.13

# Windows
# Download from https://www.python.org/downloads/
```

**Verify installation**:
```bash
python --version  # Should be 3.13+
```

### 2. PostgreSQL 13+

```bash
psql --version  # Should be 13 or later
```

### 3. Development Tools (recommended)

```bash
# Virtual environment
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate

# Package manager
pip install uv  # Modern Python package manager

# Code editor
# VS Code with Python extension recommended
# Or PyCharm Community Edition
```

---

## Architecture: How Starlette Fits In

FraiseQL uses a **layered architecture** where Starlette is the HTTP layer:

```
┌─────────────────────────────────────────┐
│  Your Code                              │
│  • Python GraphQL Types                 │
│  • Python Resolvers                     │
│  • Business Logic                       │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│  Starlette HTTP Server (Python)         │
│  • HTTP request handling (ASGI)         │
│  • GraphQL query parsing                │
│  • Response building                    │
│  • WebSocket management                 │
│  • Middleware pipeline                  │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│  FraiseQL Rust Pipeline                 │
│  • Query execution                      │
│  • Mutation processing                  │
│  • Subscription handling                │
│  • Caching                              │
│  • Field resolution                     │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│  PostgreSQL Database                    │
└──────────────────────────────────────────┘
```

**Key Point**: Your Python code doesn't change! You write types and resolvers exactly as before. Starlette handles the HTTP layer transparently.

---

## Hello World: Your First Starlette Server

Let's build a minimal working GraphQL API with Starlette.

### Step 1: Create Project Directory

```bash
mkdir my-graphql-api
cd my-graphql-api

# Create virtual environment
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate
```

### Step 2: Create requirements.txt

```text
fraiseql==2.0.0
starlette==0.36.0
uvicorn==0.28.0
pydantic==2.5.0
sqlalchemy==2.0.0
psycopg[binary]==3.17.0
python-dotenv==1.0.0
```

### Step 3: Install Dependencies

```bash
pip install -r requirements.txt
```

### Step 4: Create Your Schema (Python)

```python
# schema.py
import fraiseql
from typing import Optional

@fraiseql.type
class User:
    """A user in the system.

    Fields:
        id: Unique identifier
        name: User's full name
        email: User's email address
    """
    id: fraiseql.ID
    name: str
    email: str

@fraiseql.query
class Query:
    @fraiseql.resolve()
    async def users(self, info) -> list[User]:
        """Get all users"""
        # This will query your database via the Rust pipeline
        pass

    @fraiseql.resolve()
    async def user(self, info, id: fraiseql.ID) -> Optional[User]:
        """Get a user by ID"""
        pass

# Build your schema
schema = fraiseql.build_schema(Query)
```

### Step 5: Create Your Starlette Server

```python
# main.py
from starlette.applications import Starlette
from starlette.routing import Route, Mount
from starlette.responses import JSONResponse
from starlette.requests import Request
import json
from schema import schema

async def graphql_handler(request: Request):
    """Handle GraphQL queries"""
    data = await request.json()

    result = await schema.execute(
        query=data.get("query"),
        variable_values=data.get("variables"),
        operation_name=data.get("operationName"),
    )

    return JSONResponse({
        "data": result.data,
        "errors": result.errors,
    })

async def health_check(request: Request):
    """Health check endpoint"""
    return JSONResponse({"status": "ok"})

# Create application
routes = [
    Route("/graphql", graphql_handler, methods=["POST"]),
    Route("/health", health_check, methods=["GET"]),
]

app = Starlette(routes=routes)
```

### Step 6: Run Your Server

```bash
# Run with uvicorn (development)
uvicorn main:app --reload

# Output:
# INFO:     Uvicorn running on http://127.0.0.1:8000
# INFO:     Application startup complete
```

**Access your server**:
- GraphQL endpoint: `http://localhost:8000/graphql`
- Health check: `http://localhost:8000/health`

### Step 7: Test Your Server

**Test with curl**:
```bash
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name email } }"}'
```

**Test with GraphQL client**:
- Open http://localhost:8000/graphql in your browser
- Or use GraphQL Playground, Insomnia, or Postman

**Expected response**:
```json
{
  "data": {
    "users": [
      {"id": "1", "name": "Alice", "email": "alice@example.com"},
      {"id": "2", "name": "Bob", "email": "bob@example.com"}
    ]
  },
  "errors": null
}
```

---

## Development Workflow

### Auto-reload on Changes

The `--reload` flag enables auto-reload:
```bash
uvicorn main:app --reload
```

Your server automatically restarts when you save changes.

### Testing Your GraphQL API

**Option 1: curl (command line)**
```bash
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ users { id name } }",
    "variables": {},
    "operationName": null
  }'
```

**Option 2: Python Interactive Shell**
```bash
python

>>> from schema import schema
>>> import asyncio
>>>
>>> result = asyncio.run(schema.execute("{ users { id name } }"))
>>> print(result.data)
```

**Option 3: GraphQL Client (Recommended)**
- **Apollo Sandbox**: https://sandbox.apollo.dev (browser-based)
- **Insomnia**: https://insomnia.rest/ (API client)
- **Postman**: https://www.postman.com/ (API client)
- **GraphQL Playground**: Built into many tools

**Option 4: VS Code Extension**
- Install "REST Client" extension
- Create `requests.graphql` file
- Make requests directly from editor

### Debugging

**View detailed logging**:
```bash
# More verbose output
LOGGING_LEVEL=DEBUG uvicorn main:app --reload

# Or in code:
import logging
logging.basicConfig(level=logging.DEBUG)
```

**Use Python debugger**:
```bash
# Add breakpoint in code
async def graphql_handler(request: Request):
    breakpoint()  # ← Pause execution here
    # ...

# Run with debugger
python -m pdb main.py
```

**VS Code Debugging**:
```json
// .vscode/launch.json
{
  "version": "0.2.0",
  "configurations": [
    {
      "name": "Python: Starlette",
      "type": "python",
      "request": "launch",
      "module": "uvicorn",
      "args": ["main:app", "--reload"],
      "jinja": true
    }
  ]
}
```

---

## Common Setup Issues

### Issue 1: "ModuleNotFoundError: No module named 'starlette'"

**Problem**: Package not installed

**Solution**:
```bash
# Ensure virtual environment is active
source venv/bin/activate  # On Windows: venv\Scripts\activate

# Install dependencies
pip install -r requirements.txt

# Or individual package
pip install starlette
```

### Issue 2: "Address already in use"

**Problem**: Port 8000 is taken

**Solution**:
```bash
# Use different port
uvicorn main:app --port 8001

# Or kill existing process
lsof -i :8000
kill -9 <PID>
```

### Issue 3: "Cannot find module 'fraiseql'"

**Problem**: FraiseQL not installed

**Solution**:
```bash
pip install fraiseql
```

**Or in requirements.txt**:
```text
fraiseql>=2.0.0
```

### Issue 4: "Async function not running"

**Problem**: Forgetting to use async/await

**Before**:
```python
def graphql_handler(request: Request):  # ❌ Not async
    result = schema.execute(...)  # Can't await here
```

**After**:
```python
async def graphql_handler(request: Request):  # ✅ Async
    result = await schema.execute(...)  # Can await
```

### Issue 5: "Cannot import from schema"

**Problem**: Schema file not found or incorrect path

**Ensure schema.py exists**:
```bash
ls -la schema.py

# Check imports
python -c "from schema import schema; print(schema)"
```

---

## Python Knowledge Requirements

### If You're New to Python

No worries! Starlette is beginner-friendly.

**Essential concepts**:
1. **Async/await** - Asynchronous programming (watch a quick video, 10 min)
2. **Decorators** - Function wrappers (e.g., `@app.route`) (5 min)
3. **Type hints** - Optional but recommended (e.g., `id: str`) (5 min)
4. **Context managers** - `with` statement (2 min)

**Learning resources**:
- Official Python tutorial: https://docs.python.org/3/
- Async Python guide: https://realpython.com/async-io-python/
- Time investment: 1-2 weeks for basics

**Practical approach**:
- Start with examples
- Use IDE hints
- Look up syntax as needed
- Grow from there

### If You Know Python Already

Excellent! You'll find Starlette very intuitive.

**Key Starlette concepts**:
- **Starlette app** - The ASGI application
- **Routes** - Map URLs to handlers
- **Handlers** - Async functions that handle requests
- **Middleware** - Intercept requests/responses
- **Request/Response** - Standard ASGI objects

All follow standard Python patterns.

---

## Starlette vs. FastAPI vs. Axum

### Quick Comparison

| Feature | Starlette | FastAPI | Axum |
|---------|-----------|---------|------|
| **Language** | Python | Python | Rust |
| **Speed** | Very fast | Fast | Fastest |
| **Setup** | Simple | Simple | Moderate |
| **Code size** | Small | Medium | Variable |
| **Dependencies** | Minimal | Medium | None (built-in) |
| **Learning curve** | Easy | Easy | Moderate |
| **Performance** | 5-10K req/s | 3-8K req/s | 50K+ req/s |

### When to Choose Each

**Starlette**:
- Team is pure Python
- Want minimal overhead
- Need real-time (WebSocket)
- Value simplicity

**FastAPI**:
- Need automatic docs
- Have existing FastAPI setup
- Want batteries-included
- Legacy project

**Axum**:
- Maximum performance required
- Have Rust expertise
- Building high-frequency systems
- Can't compromise on speed

---

## Next Steps

Now that your server is running, explore:

### 📚 Continue Learning

1. **[Configuration Guide →](./02-configuration.md)** - Customize your server
   - CORS setup
   - Authentication
   - Rate limiting
   - Middleware

2. **[Production Deployment →](./03-deployment.md)** - Deploy to production
   - Docker containerization
   - Kubernetes
   - Cloud platforms
   - Monitoring

3. **[Performance Tuning →](./04-performance.md)** - Optimize for scale
   - Connection pooling
   - Query optimization
   - Caching strategies
   - Load handling

4. **[Troubleshooting →](./05-troubleshooting.md)** - Common issues
   - Performance problems
   - WebSocket issues
   - Database errors
   - Debugging strategies

### 🔗 Useful Resources

- **Official Starlette Docs**: https://www.starlette.io/
- **ASGI Spec**: https://asgi.readthedocs.io/
- **GraphQL Spec**: https://spec.graphql.org/
- **FraiseQL Docs**: See main documentation

### 💡 Tips for Success

1. **Start small** - Build and test incrementally
2. **Use Python features** - List comprehensions, context managers, async
3. **Read examples** - Copy and modify existing code
4. **Use IDE** - Let your editor help with autocomplete
5. **Ask for help** - Python community is welcoming

---

## Congratulations! 🎉

You now have:
- ✅ Starlette HTTP server running
- ✅ GraphQL endpoint accepting queries
- ✅ Development workflow ready
- ✅ Foundation for your API

**Next step?** Configure your server for your use case → [Configuration Guide](./02-configuration.md)

---

## Quick Reference

| Task | Command |
|------|---------|
| Create virtual env | `python -m venv venv` |
| Activate venv | `source venv/bin/activate` |
| Install packages | `pip install -r requirements.txt` |
| Run server | `uvicorn main:app --reload` |
| Run on different port | `uvicorn main:app --port 8001` |
| Test GraphQL | `curl -X POST http://localhost:8000/graphql` |
| Install package | `pip install package-name` |
| Freeze requirements | `pip freeze > requirements.txt` |
| Check Python version | `python --version` |

---

## Need Help?

Having trouble?

- **Stuck on setup?** → See [Common Setup Issues](#common-setup-issues) above
- **Want to configure server?** → [Configuration Guide](./02-configuration.md)
- **Ready to deploy?** → [Production Deployment](./03-deployment.md)
- **Performance questions?** → [Performance Tuning](./04-performance.md)
- **Something broken?** → [Troubleshooting](./05-troubleshooting.md)

**Keep going!** You're building something awesome! 🚀
