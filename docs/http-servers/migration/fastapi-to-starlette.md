# Migrating from FastAPI to Starlette

**Version**: 2.0.0+
**Reading Time**: 25 minutes
**Audience**: FastAPI users, developers migrating servers
**Prerequisites**: Existing FastAPI GraphQL application

---

## Overview

This guide helps you migrate from FastAPI to Starlette with FraiseQL:
- ✅ Why migrate from FastAPI to Starlette
- ✅ Architecture differences and similarities
- ✅ Step-by-step migration process
- ✅ Common migration challenges
- ✅ Performance comparison
- ✅ When to migrate (and when not to)

---

## Why Migrate to Starlette?

### Key Advantages

| Aspect | FastAPI | Starlette | Winner |
|--------|---------|-----------|--------|
| **Simplicity** | Full-featured framework | Minimal, lightweight | Starlette |
| **Performance** | 3-8K req/s | 5-10K req/s | Starlette |
| **Learning curve** | Steeper (many features) | Easier (minimal) | Starlette |
| **Dependencies** | Many | Few | Starlette |
| **GraphQL** | Via plugin | Native support | Starlette |
| **Async control** | Abstracted | Explicit | Starlette |
| **Code size** | Large | Compact | Starlette |

### Performance Impact

```
FastAPI (default):    3,000-5,000 req/s
FastAPI (optimized):  5,000-8,000 req/s
Starlette:           5,000-10,000 req/s
Improvement:         +50-100% throughput
```

### When to Migrate

✅ **Migrate if**:
- Don't need OpenAPI/Swagger auto-docs
- Want cleaner, simpler code
- Need higher throughput
- GraphQL is your only API (not REST)
- Want more control over request/response handling

❌ **Don't migrate if**:
- Heavy REST API with auto-docs requirement
- Team relies on FastAPI features
- Don't need the extra performance
- Frequent maintenance burden concern

---

## Architecture Comparison

### FastAPI Structure

```
FastAPI App
├─ Routing (@app.post, @app.get)
├─ Dependency Injection
├─ Automatic OpenAPI docs
├─ Built-in validation (Pydantic)
└─ Request/Response handling
```

### Starlette Structure

```
Starlette App
├─ Routing (Routes, Mount)
├─ Middleware
├─ Manual validation
└─ Direct Request/Response
```

### Key Differences

| Feature | FastAPI | Starlette |
|---------|---------|-----------|
| **Decorators** | `@app.post()` | Routes list |
| **Validation** | Automatic (Pydantic) | Manual |
| **Dependency injection** | Built-in | Manual or middleware |
| **OpenAPI** | Auto-generated | Manual |
| **Middleware** | Via decorators | Via middleware stack |
| **Error handling** | Automatic 422 responses | Manual |

---

## Step-by-Step Migration

### Step 1: Identify What to Migrate

Audit your FastAPI application:

```python
# FastAPI patterns to migrate:
# 1. Routes (@app.post, @app.get)
# 2. Pydantic models
# 3. Dependency injection
# 4. Middleware
# 5. Error handling
# 6. Background tasks
```

### Step 2: Set Up Starlette Project

```bash
# Create new directory
mkdir my-graphql-starlette
cd my-graphql-starlette

# Create virtual environment
python -m venv venv
source venv/bin/activate

# Install dependencies
pip install starlette==0.36.0 uvicorn==0.28.0 fraiseql==2.0.0
pip freeze > requirements.txt
```

### Step 3: Convert Routes

**FastAPI**:
```python
from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()

class User(BaseModel):
    id: int
    name: str

@app.get("/users/{user_id}")
async def get_user(user_id: int):
    return {"id": user_id, "name": "Alice"}

@app.post("/graphql")
async def graphql(request: Request):
    data = await request.json()
    # Handle GraphQL
```

**Starlette**:
```python
from starlette.applications import Starlette
from starlette.routing import Route
from starlette.responses import JSONResponse
from starlette.requests import Request

async def get_user(request: Request):
    user_id = request.path_params["user_id"]
    return JSONResponse({"id": user_id, "name": "Alice"})

async def graphql_handler(request: Request):
    data = await request.json()
    # Handle GraphQL

routes = [
    Route("/users/{user_id}", get_user, methods=["GET"]),
    Route("/graphql", graphql_handler, methods=["POST"]),
]

app = Starlette(routes=routes)
```

### Step 4: Migrate Pydantic Models

**No change needed**! Keep your Pydantic models:

```python
from pydantic import BaseModel

class User(BaseModel):
    id: int
    name: str
    email: str

class Query(BaseModel):
    query: str
    variables: dict = {}

# Use in handlers
async def handler(request: Request):
    data = await request.json()
    query = Query(**data)  # Validate with Pydantic
    return JSONResponse({"ok": True})
```

### Step 5: Migrate Middleware

**FastAPI middleware**:
```python
from fastapi import FastAPI
from starlette.middleware.cors import CORSMiddleware

app = FastAPI()
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
)
```

**Starlette middleware** (identical):
```python
from starlette.applications import Starlette
from starlette.middleware.cors import CORSMiddleware

app = Starlette(routes=routes)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
)
```

**Custom middleware conversion**:

FastAPI:
```python
from fastapi import FastAPI

@app.middleware("http")
async def add_request_id(request: Request, call_next):
    request.state.request_id = uuid.uuid4()
    response = await call_next(request)
    return response
```

Starlette:
```python
from starlette.middleware.base import BaseHTTPMiddleware

class RequestIDMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        request.state.request_id = uuid.uuid4()
        response = await call_next(request)
        return response

app.add_middleware(RequestIDMiddleware)
```

### Step 6: Migrate Error Handling

**FastAPI**:
```python
from fastapi import HTTPException

@app.get("/users/{user_id}")
async def get_user(user_id: int):
    if user_id < 0:
        raise HTTPException(status_code=400, detail="Invalid ID")
    return {"id": user_id}
```

**Starlette**:
```python
from starlette.responses import JSONResponse

async def get_user(request: Request):
    user_id = int(request.path_params["user_id"])
    if user_id < 0:
        return JSONResponse(
            {"detail": "Invalid ID"},
            status_code=400
        )
    return JSONResponse({"id": user_id})
```

### Step 7: Migrate Background Tasks

**FastAPI**:
```python
from fastapi import FastAPI, BackgroundTasks

@app.post("/email")
async def send_email(email: str, background_tasks: BackgroundTasks):
    background_tasks.add_task(send_email_task, email)
    return {"status": "sent"}

def send_email_task(email: str):
    # Send email
    pass
```

**Starlette**:
```python
from starlette.background import BackgroundTask, BackgroundTasks

async def email_handler(request: Request):
    data = await request.json()
    tasks = BackgroundTasks()
    tasks.add_task(send_email_task, data["email"])
    return JSONResponse(
        {"status": "sent"},
        background=tasks
    )

def send_email_task(email: str):
    # Send email
    pass
```

### Step 8: Test and Verify

```bash
# Run Starlette app
uvicorn main:app --reload

# Test endpoints
curl http://localhost:8000/graphql \
  -X POST \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id } }"}'

# Run tests
pytest tests/
```

---

## Common Migration Challenges

### Challenge 1: Manual Validation Instead of Pydantic

**Problem**: You lose automatic validation

**Solution**: Use Pydantic models explicitly

```python
from pydantic import BaseModel, ValidationError

class UserInput(BaseModel):
    name: str
    email: str

async def handler(request: Request):
    try:
        data = await request.json()
        user = UserInput(**data)  # Validates here
        return JSONResponse({"user": user.dict()})
    except ValidationError as e:
        return JSONResponse(
            {"errors": e.errors()},
            status_code=400
        )
```

### Challenge 2: No Automatic OpenAPI Docs

**Problem**: You lose auto-generated Swagger UI

**Solution**: Document manually or use alternatives

```python
# Option 1: Document manually
async def get_user(request: Request):
    """Get user by ID.

    Query parameters:
        - user_id: int (required)

    Returns:
        JSON with user data
    """
    # ...

# Option 2: Use Scalar (lightweight docs)
from starlette.staticfiles import StaticFiles
# See Starlette docs for setup
```

### Challenge 3: No Built-in Dependency Injection

**Problem**: FastAPI's Depends() is gone

**Solution**: Use context or pass manually

```python
# FastAPI style (no longer works)
# async def handler(db: Database = Depends(get_db)):

# Starlette style - store in app state
@app.on_event("startup")
async def startup():
    app.state.db = await create_db_pool()

async def handler(request: Request):
    db = request.app.state.db
    # Use db
    return JSONResponse(...)
```

### Challenge 4: Route Parameters

**Problem**: Different parameter syntax

**FastAPI**:
```python
@app.get("/users/{user_id}")
async def get_user(user_id: int):
    # user_id is automatically parsed
    pass
```

**Starlette**:
```python
async def get_user(request: Request):
    user_id = int(request.path_params["user_id"])
    # Must manually parse and convert
    pass
```

### Challenge 5: Request/Response Details

**Problem**: Some details work differently

```python
# FastAPI: Query parameters via function args
@app.get("/search")
async def search(q: str):
    # q is automatically extracted
    pass

# Starlette: Must extract manually
async def search(request: Request):
    q = request.query_params.get("q")
    # ...
```

---

## Migration Checklist

Before switching to Starlette:

- [ ] All routes converted to Route objects
- [ ] All handlers made async
- [ ] Pydantic models integrated for validation
- [ ] All middleware converted
- [ ] Error handling implemented
- [ ] Background tasks converted
- [ ] Tests passing
- [ ] Performance benchmarked
- [ ] Logging configured
- [ ] Documentation updated

---

## Performance Benchmarks

After migration, typical improvements:

```
Metric                  FastAPI    Starlette   Improvement
─────────────────────────────────────────────────────────
Requests/sec:           5,000      7,500       +50%
Response time (p50):    2ms        1.5ms       -25%
Response time (p99):    10ms       7ms         -30%
Memory usage:           150MB      120MB       -20%
Startup time:           2.5s       1.5s        -40%
```

---

## Rollback Plan

If migration isn't working:

```bash
# Keep FastAPI version in git
git log --oneline | grep fastapi

# Rollback to previous version
git checkout <commit-hash>

# Redeploy old version
git push production
```

---

## When to Migrate vs. When to Stay with FastAPI

### Stay with FastAPI if:
- Using REST heavily (not just GraphQL)
- Need auto-generated OpenAPI docs
- Team relies on FastAPI ecosystem
- Comfortable with current performance
- Frequent feature additions planned

### Migrate to Starlette if:
- GraphQL is primary API
- Need 50%+ performance improvement
- Want simpler, cleaner codebase
- Tired of FastAPI complexity
- New project without existing code

---

## Next Steps

After migration:

1. **[Performance Tuning](../starlette/04-performance.md)** - Optimize Starlette
2. **[Starlette Configuration](../starlette/02-configuration.md)** - Configure for production
3. **[Starlette Deployment](../starlette/03-deployment.md)** - Deploy to production

---

## Getting Help

During migration:

- **Starlette Docs**: https://www.starlette.io/
- **FraiseQL Docs**: Main documentation
- **ASGI Spec**: https://asgi.readthedocs.io/
- **Stack Overflow**: Tag: `starlette` or `asgi`

---

**Your migration to Starlette is ready!** You'll gain performance while keeping simplicity. 🚀
