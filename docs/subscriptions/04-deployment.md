# Deployment Guide - GraphQL Subscriptions

Production deployment strategies and best practices.

---

## Quick Start Deployments

### Development (Local Machine)

```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# Memory event bus - perfect for dev
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)

# Start your FastAPI app
# uvicorn app:app --reload
```

**Characteristics**:
- Simple single-process setup
- No external dependencies
- Good for testing and learning
- Events lost on restart

---

### Single Server Production

```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# Memory event bus still works fine
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)

# Deploy with:
# - uvicorn with multiple workers
# - gunicorn with multiple processes
# - Docker container
```

**Characteristics**:
- Supports 10,000+ subscriptions
- Handles 50,000+ events/sec
- No additional infrastructure
- Subscriptions tied to this server instance

---

### Multi-Server Production

```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# Redis event bus for multi-server
config = _fraiseql_rs.PyEventBusConfig.redis(
    host="redis.prod.example.com",
    port=6379,
    db=0,
    password=os.getenv("REDIS_PASSWORD")
)
manager = SubscriptionManager(config)
```

**Characteristics**:
- Scales across multiple servers
- Events shared via Redis pub/sub
- Requires sticky sessions (same server per connection)
- Supports 100,000+ subscriptions

---

## Event Bus Configuration

### Memory Event Bus

**Best For**: Development, single-server production

```python
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)
```

**Advantages**:
- ✅ Fastest (in-process, no network)
- ✅ No external dependencies
- ✅ Simple setup

**Disadvantages**:
- ❌ Single server only
- ❌ Subscriptions lost on restart
- ❌ No horizontal scaling

---

### Redis Event Bus

**Best For**: Multi-server deployments, high availability

```python
from fraiseql import _fraiseql_rs

config = _fraiseql_rs.PyEventBusConfig.redis(
    host="redis.prod.example.com",
    port=6379,
    db=0,
    password="secure_password",
    ssl=True
)
manager = SubscriptionManager(config)
```

**Configuration Options**:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `host` | str | "localhost" | Redis server hostname |
| `port` | int | 6379 | Redis server port |
| `db` | int | 0 | Redis database number |
| `password` | str \| None | None | Redis password |
| `ssl` | bool | False | Use SSL/TLS |

**Advantages**:
- ✅ Multi-server support
- ✅ High throughput (>50k events/sec)
- ✅ Horizontal scaling
- ✅ Optional persistence

**Disadvantages**:
- ❌ Additional infrastructure (Redis)
- ❌ Slightly higher latency (~5ms vs <1ms)
- ❌ Network dependency

**Setup Example**:

```bash
# Using Docker
docker run -d \
  --name redis \
  -p 6379:6379 \
  redis:latest

# Using Docker Compose
services:
  redis:
    image: redis:latest
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data
```

---

### PostgreSQL Event Bus

**Best For**: Deployments with existing PostgreSQL, persistence required

```python
from fraiseql import _fraiseql_rs

config = _fraiseql_rs.PyEventBusConfig.postgresql(
    connection_string="postgresql://user:password@postgres.prod.example.com:5432/fraiseql"
)
manager = SubscriptionManager(config)
```

**Connection String Format**:

```
postgresql://[user[:password]@][host][:port][/database]
```

**Examples**:

```python
# Local development
"postgresql://localhost/fraiseql"

# With credentials
"postgresql://app_user:secure_pass@localhost:5432/fraiseql"

# Production with SSL
"postgresql://app_user:secure_pass@postgres.prod.example.com:5432/fraiseql?sslmode=require"
```

**Advantages**:
- ✅ Multi-server support via LISTEN/NOTIFY
- ✅ Built-in persistence
- ✅ No additional services (if using existing DB)
- ✅ Good for audit logging

**Disadvantages**:
- ❌ Lower throughput than Redis (~10k events/sec)
- ❌ Higher latency (~10ms)
- ❌ Database connection pool scaling

**Setup Example**:

```sql
-- No special setup needed!
-- PostgreSQL's LISTEN/NOTIFY works out of the box
-- Just ensure your app has database access
```

---

## Docker Deployment

### Single Container

```dockerfile
FROM python:3.13-slim

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

CMD ["uvicorn", "app:app", "--host", "0.0.0.0", "--port", "8000"]
```

**Build and Run**:

```bash
docker build -t fraiseql-subscriptions .
docker run -p 8000:8000 fraiseql-subscriptions
```

---

### Docker Compose (Multi-Server)

```yaml
version: '3.8'

services:
  redis:
    image: redis:latest
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data

  app1:
    build: .
    environment:
      REDIS_HOST: redis
      REDIS_PORT: 6379
    ports:
      - "8001:8000"
    depends_on:
      - redis

  app2:
    build: .
    environment:
      REDIS_HOST: redis
      REDIS_PORT: 6379
    ports:
      - "8002:8000"
    depends_on:
      - redis

  app3:
    build: .
    environment:
      REDIS_HOST: redis
      REDIS_PORT: 6379
    ports:
      - "8003:8000"
    depends_on:
      - redis

  nginx:
    image: nginx:latest
    ports:
      - "80:80"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
    depends_on:
      - app1
      - app2
      - app3

volumes:
  redis_data:
```

**nginx.conf** (sticky sessions):

```nginx
upstream fraiseql_servers {
    least_conn;  # Minimize connection count
    server app1:8000;
    server app2:8000;
    server app3:8000;
}

server {
    listen 80;

    location /graphql/subscriptions {
        proxy_pass http://fraiseql_servers;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        # Sticky sessions via IP hash
        ip_hash;
    }

    location / {
        proxy_pass http://fraiseql_servers;
    }
}
```

---

## Kubernetes Deployment

### Basic Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-subscriptions
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql
  template:
    metadata:
      labels:
        app: fraiseql
    spec:
      containers:
      - name: fraiseql
        image: fraiseql-subscriptions:latest
        ports:
        - containerPort: 8000
        env:
        - name: REDIS_HOST
          value: redis
        - name: REDIS_PORT
          value: "6379"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 10

---
apiVersion: v1
kind: Service
metadata:
  name: fraiseql
spec:
  type: LoadBalancer
  selector:
    app: fraiseql
  ports:
  - port: 80
    targetPort: 8000
    protocol: TCP
  sessionAffinity: ClientIP  # Sticky sessions
```

### With Redis

```yaml
apiVersion: v1
kind: Service
metadata:
  name: redis
spec:
  ports:
  - port: 6379
  clusterIP: None
  selector:
    app: redis

---
apiVersion: v1
kind: Pod
metadata:
  name: redis
  labels:
    app: redis
spec:
  containers:
  - name: redis
    image: redis:latest
    ports:
    - containerPort: 6379
```

---

## Load Balancing & Sticky Sessions

### Why Sticky Sessions?

Each server maintains its own in-memory subscription state. When a client reconnects, it must reach the same server.

```
Client 1 connects to Server A
  ├─ Creates subscription "sub_123" in Server A memory
  └─ Must always route to Server A

Client 1 reconnects
  ├─ If routed to Server B → subscription lost!
  └─ Must route back to Server A → subscription found
```

### Implementing Sticky Sessions

**Nginx (IP Hash)**:
```nginx
upstream backend {
    ip_hash;
    server server1.example.com;
    server server2.example.com;
    server server3.example.com;
}
```

**HAProxy (Cookie-Based)**:
```
balance roundrobin
cookie SERVERID insert indirect nocache
server server1 192.168.1.1:8000 cookie server1
server server2 192.168.1.2:8000 cookie server2
server server3 192.168.1.3:8000 cookie server3
```

**AWS ALB (Client IP)**:
```
Target Group Attributes:
- Stickiness: Enabled
- Duration: 1 day
```

---

## Performance Tuning

### Environment Variables

```bash
# Python async event loop tuning
export PYTHONUNBUFFERED=1
export PYTHONIOENCODING=utf-8

# Event bus configuration
export REDIS_HOST=redis.example.com
export REDIS_PORT=6379

# Application settings
export WORKERS=4  # For gunicorn/uvicorn
```

### Uvicorn Configuration

```python
import uvicorn

config = uvicorn.Config(
    app="app:app",
    host="0.0.0.0",
    port=8000,
    workers=4,              # Number of worker processes
    loop="uvloop",          # Fast event loop
    http="httptools",       # Fast HTTP parser
    interface="auto",       # Auto-detect best interface
)

server = uvicorn.Server(config)
asyncio.run(server.serve())
```

### Resource Allocation

```
Per instance:
- Memory: 256MB-512MB
- CPU: 0.5-1 CPU
- Subscriptions: 1000-10000

Scaling rule:
- 1000 subscriptions: 1 instance
- 10,000 subscriptions: 10 instances
- 100,000 subscriptions: 100 instances + Redis cluster
```

---

## Monitoring & Observability

### Health Check Endpoint

```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/health")
async def health():
    return {"status": "healthy"}
```

### Metrics Collection

```python
from prometheus_client import Counter, Histogram
import time

# Metrics
subscriptions_created = Counter('subscriptions_created', 'Total subscriptions')
events_published = Counter('events_published', 'Total events published')
resolver_duration = Histogram('resolver_duration_seconds', 'Resolver execution time')

@app.websocket("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    # Track subscription creation
    subscriptions_created.inc()

    # Track resolver performance
    start = time.time()
    resolver_result = await resolver(event, variables)
    resolver_duration.observe(time.time() - start)
```

### Logging

```python
import logging

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)

logger = logging.getLogger(__name__)

# Log important events
logger.info(f"Subscription created: {subscription_id}")
logger.warning(f"Rate limit exceeded for user: {user_id}")
logger.error(f"Resolver failed for subscription: {subscription_id}")
```

---

## Security Checklist

**Network**:
- [ ] Use WSS (WebSocket Secure) in production
- [ ] Use HTTPS for REST endpoints
- [ ] Firewall Redis/PostgreSQL (not public)

**Authentication**:
- [ ] Verify JWT tokens on subscription creation
- [ ] Validate user_id and tenant_id
- [ ] Refresh tokens as needed

**Rate Limiting**:
- [ ] Enable built-in rate limiting
- [ ] Monitor for abuse patterns
- [ ] Set appropriate quotas per user

**Data**:
- [ ] Don't send sensitive data through events (filter in resolver)
- [ ] Validate all event data in resolver
- [ ] Log sensitive operations

---

## Troubleshooting Deployment

### Subscriptions Lost on Restart

**Problem**: Subscriptions disappear when server restarts

**Solution**: Use Redis event bus with persistent connection tracking

```python
config = _fraiseql_rs.PyEventBusConfig.redis(
    host="redis.example.com",
    port=6379
)
```

### High Latency

**Problem**: Events take >10ms to deliver

**Check**:
1. Event bus configuration (memory vs Redis vs PostgreSQL)
2. Resolver function performance
3. Network latency (if using Redis)

**Solution**:
```python
# Monitor resolver execution
async def monitored_resolver(event, variables):
    import time
    start = time.time()
    result = await process_event(event)
    if time.time() - start > 0.1:
        logger.warning("Slow resolver")
    return result
```

### Memory Leaks

**Problem**: Memory usage grows over time

**Check**:
1. Resolver functions creating unbounded data
2. Event queue not clearing properly
3. Subscription cleanup not happening

**Solution**:
```python
# Always cleanup on disconnect
try:
    # Handle subscriptions
finally:
    for sub_id in subscriptions:
        await manager.complete_subscription(sub_id)
```

---

## Production Checklist

- [ ] Event bus configured (not memory)
- [ ] Sticky sessions enabled on load balancer
- [ ] Health check endpoint working
- [ ] Monitoring/logging configured
- [ ] Security: WSS enabled
- [ ] Security: JWT validation in place
- [ ] Resource limits set
- [ ] Graceful shutdown handling
- [ ] Backup/recovery plan for data
- [ ] Performance tested with expected load

---

See troubleshooting guide (`06-troubleshooting.md`) for common issues and solutions.
