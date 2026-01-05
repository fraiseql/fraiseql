# Troubleshooting: Starlette

**Version**: 2.0.0+
**Reading Time**: 30 minutes
**Audience**: Backend developers, DevOps engineers
**Prerequisites**: Any Starlette guide ([Getting Started](./01-getting-started.md), [Configuration](./02-configuration.md), [Deployment](./03-deployment.md), [Performance](./04-performance.md))

---

## Overview

This guide helps you diagnose and fix common Starlette issues:
- ✅ Import and installation errors
- ✅ Startup and configuration errors
- ✅ Runtime errors and exceptions
- ✅ Async/await issues
- ✅ Database connection problems
- ✅ Performance degradation
- ✅ WebSocket issues
- ✅ Debugging strategies

---

## Installation & Import Errors

### Error: "ModuleNotFoundError: No module named 'starlette'"

**Cause**: Starlette not installed

**Solution**:
```bash
# Ensure virtual environment is active
source venv/bin/activate

# Install Starlette
pip install starlette

# Or from requirements
pip install -r requirements.txt
```

**Verify installation**:
```bash
python -c "import starlette; print(starlette.__version__)"
```

---

### Error: "ImportError: cannot import name 'Starlette'"

**Cause**: Wrong import path

**Before**:
```python
from starlette import Starlette  # ❌ Wrong
```

**After**:
```python
from starlette.applications import Starlette  # ✅ Correct
```

---

### Error: "ModuleNotFoundError: No module named 'fraiseql'"

**Problem**: FraiseQL not installed

**Solution**:
```bash
pip install fraiseql
```

**Or add to requirements.txt**:
```text
fraiseql>=2.0.0
starlette==0.36.0
uvicorn==0.28.0
```

---

## Startup Errors

### Error: "Address already in use"

**Cause**: Port 8000 already in use

**Solution**:
```bash
# Use different port
uvicorn main:app --port 8001

# Or kill existing process
lsof -i :8000
kill -9 <PID>

# Or find what's using the port
netstat -tulpn | grep 8000
```

---

### Error: "ERROR: Application startup failed"

**Cause**: Error during app initialization

**Diagnosis**:
```bash
# Run with verbose logging
LOGGING_LEVEL=debug uvicorn main:app --reload

# Or in code
import logging
logging.basicConfig(level=logging.DEBUG)
```

**Common causes**:

1. **Missing environment variables**
```python
import os

DATABASE_URL = os.environ.get("DATABASE_URL")
if not DATABASE_URL:
    raise ValueError("DATABASE_URL not set")
```

2. **Database connection failure**
```python
@app.on_event("startup")
async def startup():
    try:
        # Create pool
        app.state.db = await create_pool()
    except Exception as e:
        print(f"Failed to connect to database: {e}")
        raise
```

---

### Error: "No such file or directory" for .env file

**Cause**: .env file not found

**Solution**:
```bash
# Create .env file
cat > .env << EOF
DATABASE_URL=postgresql://user:pass@localhost/dbname
JWT_SECRET=your-secret-key
EOF

# Load it in code
from dotenv import load_dotenv
load_dotenv()
```

---

## Async/Await Errors

### Error: "TypeError: object Request can't be used in 'await' expression"

**Cause**: Forgetting to await async function

**Before**:
```python
async def handler(request: Request):
    data = request.json()  # ❌ Missing await
```

**After**:
```python
async def handler(request: Request):
    data = await request.json()  # ✅ Correct
```

---

### Error: "TypeError: non-awaitable ... returned"

**Cause**: Handler not declared as async

**Before**:
```python
def graphql_handler(request: Request):  # ❌ Not async
    result = schema.execute(...)
    return JSONResponse(result)
```

**After**:
```python
async def graphql_handler(request: Request):  # ✅ Async
    result = await schema.execute(...)
    return JSONResponse(result)
```

---

### Error: "RuntimeError: no running event loop"

**Cause**: Trying to use async code in non-async context

**Example problem**:
```python
# ❌ This fails
import asyncio

def sync_function():
    result = asyncio.run(some_async_function())  # Wrong context
```

**Solution**:
```python
# ✅ Use async all the way
async def handler(request: Request):
    result = await some_async_function()
    return JSONResponse(result)
```

---

## Runtime Errors

### Error: "JSONDecodeError: Expecting value"

**Cause**: Invalid JSON in request

**Diagnosis**:
```bash
# Test with curl
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id } }"}'
```

**Common issues**:

1. **Missing Content-Type header**
```bash
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id } }"}'
```

2. **Malformed JSON**
```python
# ❌ Invalid JSON
-d '{"query": "{ users { id }" }'  # Missing closing brace

# ✅ Valid JSON
-d '{"query": "{ users { id } }"}'
```

---

### Error: "KeyError" or "TypeError" in GraphQL handler

**Cause**: Missing required field in request

**Fix**:
```python
async def graphql_handler(request: Request):
    try:
        data = await request.json()
    except ValueError:
        return JSONResponse(
            {"error": "Invalid JSON"},
            status_code=400
        )

    # Use .get() with defaults
    query = data.get("query")
    variables = data.get("variables", {})
    operation = data.get("operationName")

    if not query:
        return JSONResponse(
            {"error": "query is required"},
            status_code=400
        )
```

---

### Error: "500 Internal Server Error" in production

**Diagnosis**:

1. **Check logs**
```bash
# Docker
docker logs container-id

# Kubernetes
kubectl logs -f deployment/graphql-api

# Local
tail -f app.log
```

2. **Enable debug logging**
```python
import logging
logging.basicConfig(level=logging.DEBUG)

# Or set env var
STARLETTE_DEBUG=true uvicorn main:app
```

---

## Database Connection Errors

### Error: "Cannot connect to PostgreSQL"

**Diagnosis**:
```bash
# Test connection directly
psql $DATABASE_URL

# Check if PostgreSQL is running
pg_isready -h localhost -p 5432

# Check database URL format
echo $DATABASE_URL
```

**Common causes**:

1. **Wrong DATABASE_URL format**
```bash
# ❌ Wrong
postgresql://localhost/db

# ✅ Correct with auth
postgresql://user:password@localhost:5432/dbname
```

2. **PostgreSQL not running**
```bash
# Start PostgreSQL
brew services start postgresql  # macOS
sudo systemctl start postgresql # Linux
```

3. **Network/firewall issue**
```bash
# Check if port is open
nc -zv localhost 5432
telnet localhost 5432
```

---

### Error: "Too many connections"

**Cause**: Database connection pool exhausted or PostgreSQL max connections reached

**Check PostgreSQL limit**:
```sql
SHOW max_connections;  -- Default: 100
```

**Diagnosis**:
```python
# Log pool status
engine = create_engine(DATABASE_URL)
pool = engine.pool
print(f"Pool size: {pool.size()}")
print(f"Checked out: {pool.checkedout()}")
```

**Solutions**:

1. **Reduce pool size**
```python
engine = create_engine(
    DATABASE_URL,
    pool_size=10,          # Reduce from default
    max_overflow=5
)
```

2. **Increase PostgreSQL limit**
```sql
ALTER SYSTEM SET max_connections = 200;
SELECT pg_reload_conf();
```

3. **Use connection pooler (PgBouncer)**
```
Clients → PgBouncer (1000 conn) → PostgreSQL (100 conn)
```

---

### Error: "Database connection timeout"

**Cause**: Connection takes too long to establish

**Diagnosis**:
```bash
# Time connection
time psql $DATABASE_URL -c "SELECT 1"
```

**Solutions**:

1. **Increase timeout**
```python
engine = create_engine(
    DATABASE_URL,
    connect_args={"timeout": 20}  # Increase from 10
)
```

2. **Check network**
```bash
ping database-host
traceroute database-host
```

---

## Performance Issues

### Slow Requests (> 100ms per request)

**Diagnosis**:
```bash
# Enable query logging
SQLALCHEMY_ECHO=true uvicorn main:app

# Check timing
curl -w "Total: %{time_total}s\n" http://localhost:8000/graphql
```

**Common causes**:

1. **N+1 Query Problem**
```python
# ❌ Slow
users = await db.fetch("SELECT * FROM users")
for user in users:
    posts = await db.fetch("SELECT * FROM posts WHERE user_id = $1", user.id)

# ✅ Fast
users_with_posts = await db.fetch("""
    SELECT users.*, posts.*
    FROM users
    LEFT JOIN posts ON users.id = posts.user_id
""")
```

2. **Missing database indexes**
```sql
-- Add indexes
CREATE INDEX idx_posts_user_id ON posts(user_id);
CREATE INDEX idx_users_email ON users(email);
```

3. **Inefficient GraphQL query**
```graphql
# ❌ Fetches everything
{ users { id name email posts { id title comments { id text } } } }

# ✅ Only fetch needed fields
{ users { id name } }
```

---

### High Memory Usage

**Diagnosis**:
```python
import psutil
import os

process = psutil.Process(os.getpid())
mem = process.memory_info().rss / 1024 / 1024
print(f"Memory: {mem:.2f}MB")

# Monitor over time
import asyncio
while True:
    mem = process.memory_info().rss / 1024 / 1024
    print(f"Memory: {mem:.2f}MB")
    await asyncio.sleep(10)
```

**Common causes**:

1. **Unbounded cache**
```python
# ❌ Grows forever
cache = {}

# ✅ Use TTL and size limit
from functools import lru_cache
@lru_cache(maxsize=1000)
def cached_function(x):
    return x ** 2
```

2. **Unclosed connections**
```python
# ❌ Connection leaked
conn = pool.acquire()
# ... do something ...
# forgot to close!

# ✅ Properly close
async with pool.acquire() as conn:
    # ... use connection ...
    # Auto-closed here
```

---

## WebSocket Issues

### WebSocket Connection Fails

**Cause**: WebSocket endpoint not defined

**Solution**:
```python
from starlette.websockets import WebSocket

@app.websocket_route("/ws")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    try:
        while True:
            data = await websocket.receive_text()
            await websocket.send_text(f"Echo: {data}")
    except Exception as e:
        print(f"WebSocket error: {e}")
    finally:
        await websocket.close()
```

---

### WebSocket Message Loss

**Cause**: Improper error handling

**Fix**:
```python
@app.websocket_route("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()

    try:
        while True:
            data = await websocket.receive_text()

            # Execute GraphQL subscription
            result = await schema.subscribe(data)

            # Send results
            async for message in result:
                await websocket.send_json(message)

    except Exception as e:
        print(f"WebSocket error: {e}")
        await websocket.close(code=1011)  # Server error
```

---

## CORS Issues

### CORS Error in Browser

**Symptom**: Browser blocks request with CORS error

**Diagnosis**:
```bash
# Check CORS headers
curl -i -X OPTIONS http://localhost:8000/graphql \
  -H "Origin: http://localhost:3000" \
  -H "Access-Control-Request-Method: POST"
```

**Expected headers**:
```
Access-Control-Allow-Origin: http://localhost:3000
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: Content-Type, Authorization
```

**If missing**:
```python
from starlette.middleware.cors import CORSMiddleware

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_methods=["*"],
    allow_headers=["*"],
)
```

---

## Debugging Strategies

### Strategy 1: Enable Detailed Logging

```python
import logging

# Configure logging
logging.basicConfig(
    level=logging.DEBUG,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)

# Get logger for your module
logger = logging.getLogger(__name__)

# Log key points
async def handler(request: Request):
    logger.info("Handler called")
    data = await request.json()
    logger.info(f"Received query: {data.get('query')}")

    result = await schema.execute(...)
    logger.info(f"Result: {result}")
    return JSONResponse(result)
```

### Strategy 2: Use Python Debugger

```python
# Add breakpoints
async def handler(request: Request):
    breakpoint()  # Pause execution here
    data = await request.json()
    return JSONResponse(data)

# Run with debugger
python -m pdb main.py
```

### Strategy 3: Binary Search for Issues

```python
# Add logging at key points
async def handler(request: Request):
    logger.info("1. Parsing request")
    data = await request.json()

    logger.info("2. Validating query")
    query = data.get("query")

    logger.info("3. Executing query")
    result = await schema.execute(query)

    logger.info("4. Serializing response")
    response = JSONResponse(result)

    logger.info("5. Returning response")
    return response

# See which step fails/slows down
```

---

## Common Error Messages

### "RuntimeError: Event loop is running"

**Cause**: Trying to run async code in already-running event loop

**Problem**:
```python
# ❌ This fails
async def async_function():
    asyncio.run(other_async_function())  # Can't nest event loops
```

**Solution**:
```python
# ✅ Correct
async def async_function():
    return await other_async_function()  # Direct await
```

---

### "ValueError: duplicate option name"

**Cause**: Same route/option defined twice

**Check**:
```python
# ❌ Duplicate
@app.route("/graphql", methods=["POST"])
async def handler1():
    pass

@app.route("/graphql", methods=["POST"])
async def handler2():
    pass

# ✅ Unique routes
@app.route("/graphql", methods=["POST"])
async def graphql_handler():
    pass

@app.route("/health", methods=["GET"])
async def health_handler():
    pass
```

---

## Getting Help

When stuck:

1. **Read the error message carefully**
   - Python errors are descriptive
   - Include the full traceback

2. **Check the logs**
   ```bash
   # Verbose logging
   LOGGING_LEVEL=debug uvicorn main:app
   ```

3. **Search for the error**
   - Google: "[error message]"
   - GitHub issues
   - Stack Overflow

4. **Create minimal reproduction**
   ```python
   # Simplest code that shows the problem
   # Should be runnable by others
   ```

5. **Useful resources**
   - Starlette docs: https://www.starlette.io/
   - ASGI spec: https://asgi.readthedocs.io/
   - Python docs: https://docs.python.org/3/
   - FraiseQL docs: Main documentation

---

## Troubleshooting Checklist

When something breaks:

- [ ] Read error message carefully
- [ ] Enable debug logging
- [ ] Check if it worked before (git diff)
- [ ] Create minimal reproduction case
- [ ] Check external services (database, cache)
- [ ] Try reverting recent changes
- [ ] Ask for help with minimal example

---

## Next Steps

- **Need performance help?** → [Performance Tuning](./04-performance.md)
- **Back to Deployment?** → [Production Deployment](./03-deployment.md)
- **Back to Configuration?** → [Configuration](./02-configuration.md)
- **Back to Getting Started?** → [Getting Started](./01-getting-started.md)

---

**Remember**: Most issues have a simple fix. Read the error, enable logging, and search for the error message. 🚀
