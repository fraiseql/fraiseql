# FraiseQL Pool Configuration Guide

Complete guide to configuring database connections and connection pooling in FraiseQL's Rust pipeline.

**Last Updated**: January 2026
**Framework**: FraiseQL v1.9.5 with Rust Pipeline Phase 3.1

## Quick Start

### Minimal Configuration

```python
import fraiseql

# Create engine with PostgreSQL URL
engine = fraiseql.GraphQLEngine(json.dumps({
    "db": "postgresql://user:password@localhost/fraiseql"
}))
```

### Full Configuration

```python
engine = fraiseql.GraphQLEngine(json.dumps({
    "db": {
        "url": "postgresql://user:password@localhost:5432/fraiseql",
        "pool_size": 20,
        "timeout_seconds": 30
    },
    "cache": {
        "type": "memory",
        "ttl_seconds": 3600
    }
}))
```

## Connection URL Format

### PostgreSQL URL Syntax

```
postgresql://[user[:password]@][host][:port][/database]
```

or the shorter form:

```
postgres://[user[:password]@][host][:port][/database]
```

### URL Components

| Component | Required | Default | Example |
|-----------|----------|---------|---------|
| **Scheme** | Yes | - | `postgresql://` or `postgres://` |
| **User** | No | postgres | `myuser` |
| **Password** | No | - | `secretpass` |
| **Host** | No | localhost | `db.example.com` |
| **Port** | No | 5432 | `5433` |
| **Database** | No | postgres | `myapp_db` |

### URL Examples

**Local Development** (all defaults):
```
postgresql://localhost/fraiseql
```

**With Credentials**:
```
postgresql://app_user:secure_password@localhost/fraiseql
```

**Custom Port**:
```
postgresql://user:pass@localhost:5433/fraiseql
```

**Remote Host**:
```
postgresql://user:pass@db.prod.example.com:5432/production_db
```

**In Code**:
```python
config = {
    "db": "postgresql://app_user:secure_pass@db.example.com:5432/fraiseql"
}
engine = fraiseql.GraphQLEngine(json.dumps(config))
```

## Pool Configuration

### Configuration Object Format

```python
config = {
    "db": {
        "url": "postgresql://user:pass@localhost/db",
        "pool_size": 10,           # Max connections
        "timeout_seconds": 30      # Connection acquisition timeout
    }
}
```

### Pool Size

**What it is**: Maximum number of concurrent database connections in the pool.

**Default**: 10 connections

**Range**: 1-100+ (depends on database limits)

**When to adjust**:
- **Small applications**: 5-10 (default is fine)
- **Medium load**: 15-25 connections
- **High concurrency**: 30-50+ connections
- **Many servers**: 5-10 per instance (multiply by server count)

**Example - High Concurrency**:
```python
config = {
    "db": {
        "url": "postgresql://user:pass@localhost/db",
        "pool_size": 30
    }
}
```

### Connection Timeout

**What it is**: How long to wait (in seconds) when acquiring a connection from the pool.

**Default**: 30 seconds

**Range**: 1-600 seconds (1 second to 10 minutes)

**When to adjust**:
- **High latency networks**: Increase to 60+ seconds
- **Fast local networks**: Can reduce to 5-10 seconds
- **Transient issues**: Increase slightly for resilience

**Example - Long Timeout for Remote DB**:
```python
config = {
    "db": {
        "url": "postgresql://user:pass@remote-db.cloud.com/db",
        "timeout_seconds": 60
    }
}
```

### Complete Pool Configuration Options

```python
config = {
    "db": {
        # Connection URL (REQUIRED)
        "url": "postgresql://user:pass@host/db",

        # Pool sizing (OPTIONAL)
        "pool_size": 10,              # Max connections (default: 10)
        "timeout_seconds": 30         # Acquire timeout (default: 30)
    }
}
```

## Database Connection Scenarios

### Local Development

```python
config = {
    "db": "postgresql://localhost/fraiseql"
}
```

**Assumptions**:
- PostgreSQL running on localhost:5432
- User: postgres (default)
- Database: fraiseql
- Pool size: 10 (default)

### Docker Compose Setup

```python
# docker-compose.yml has PostgreSQL on postgres:5432
config = {
    "db": "postgresql://postgres:postgres@postgres:5432/fraiseql"
}
```

### AWS RDS Production

```python
config = {
    "db": {
        "url": "postgresql://app_user:SecurePassword123@fraiseql.abc123.us-east-1.rds.amazonaws.com:5432/fraiseql",
        "pool_size": 20,           # Higher for production
        "timeout_seconds": 45      # Slightly higher for cloud latency
    }
}
```

### Heroku PostgreSQL

```python
import os

# Heroku provides DATABASE_URL environment variable
database_url = os.environ.get("DATABASE_URL")

config = {
    "db": database_url
}
```

### GCP Cloud SQL

```python
config = {
    "db": {
        "url": "postgresql://fraiseql_user:CloudPassword@35.192.123.45:5432/fraiseql",
        "pool_size": 15,
        "timeout_seconds": 40
    }
}
```

### Multi-Server Setup (Load Balanced)

If running FraiseQL on multiple servers:

```python
# Each server: use smaller pool sizes, let load balancer handle scaling
config = {
    "db": {
        "url": "postgresql://user:pass@shared-db.prod.example.com/fraiseql",
        "pool_size": 8,             # Smaller pool per instance
        "timeout_seconds": 30
    }
}

# With 5 servers × 8 connections = 40 total connections to database
```

## SSL/TLS Connections

FraiseQL uses PostgreSQL's SSL/TLS support via the underlying deadpool-postgres connection pool.

### SSL Mode Support

**Supported modes**:
- `disable` - No SSL (development only)
- `prefer` - Try SSL, fall back to plaintext (default)
- `require` - Force SSL connection

**Configure via URL parameters**:
```python
# Require SSL
config = {
    "db": "postgresql://user:pass@host/db?sslmode=require"
}
```

### Production SSL Requirements

```python
config = {
    "db": {
        "url": "postgresql://fraiseql_user:SecurePass@db.prod.example.com/fraiseql",
        # SSL is required in production
    }
}
```

**Note**: Most cloud providers (AWS RDS, GCP Cloud SQL, Heroku) enforce SSL by default.

## Environment Variables

### Using DATABASE_URL

```python
import os
import json

# Common pattern: DATABASE_URL environment variable
database_url = os.environ.get("DATABASE_URL", "postgresql://localhost/fraiseql")

config = {
    "db": database_url
}

engine = fraiseql.GraphQLEngine(json.dumps(config))
```

### Configuration from Environment

```python
import os
import json

config = {
    "db": {
        "url": os.environ["DATABASE_URL"],
        "pool_size": int(os.environ.get("DB_POOL_SIZE", "10")),
        "timeout_seconds": int(os.environ.get("DB_TIMEOUT", "30"))
    }
}

engine = fraiseql.GraphQLEngine(json.dumps(config))
```

### Docker Environment

```dockerfile
# Dockerfile
ENV DATABASE_URL=postgresql://user:pass@postgres:5432/fraiseql
ENV DB_POOL_SIZE=15

WORKDIR /app
COPY . .
RUN pip install -r requirements.txt

CMD ["python", "-m", "myapp"]
```

## Troubleshooting

### Connection Refused

**Error**: `Connection refused`

**Causes**:
1. PostgreSQL not running
2. Wrong host/port
3. Firewall blocking connection

**Solutions**:
```bash
# Check if PostgreSQL is running
pg_isready -h localhost -p 5432

# Test connection manually
psql postgresql://user:pass@localhost/db

# Check firewall
telnet localhost 5432
```

**Config Fix**:
```python
# Verify URL is correct
config = {
    "db": "postgresql://postgres:postgres@localhost:5432/fraiseql"
}
```

### Connection Timeout

**Error**: `Connection timeout` or `Acquire timeout`

**Causes**:
1. Database unreachable (network issue)
2. Pool exhausted (all connections in use)
3. Timeout too short for network latency

**Solutions**:

Increase timeout for slow networks:
```python
config = {
    "db": {
        "url": "postgresql://user:pass@remote-host/db",
        "timeout_seconds": 60  # Increased from 30
    }
}
```

Check pool size if load is high:
```python
config = {
    "db": {
        "url": "postgresql://user:pass@localhost/db",
        "pool_size": 20  # Increased from 10
    }
}
```

### Authentication Failed

**Error**: `Authentication failed` or `Invalid user`

**Causes**:
1. Wrong username/password
2. User doesn't exist in database
3. User doesn't have permission to connect

**Solutions**:

Verify credentials:
```bash
psql -U myuser -d fraiseql -c "SELECT 1"
```

Update config with correct credentials:
```python
config = {
    "db": "postgresql://correct_user:correct_password@localhost/fraiseql"
}
```

Create PostgreSQL user if needed:
```sql
CREATE ROLE fraiseql_user WITH LOGIN PASSWORD 'secure_password';
GRANT CONNECT ON DATABASE fraiseql TO fraiseql_user;
```

### Database Does Not Exist

**Error**: `Database does not exist` or `FATAL: database "db" does not exist`

**Solution**:

Create the database:
```bash
createdb fraiseql
```

Or via SQL:
```sql
CREATE DATABASE fraiseql;
```

Verify URL has correct database name:
```python
config = {
    "db": "postgresql://user:pass@localhost/fraiseql"  # Correct DB name
}
```

### Pool Exhaustion Under Load

**Symptoms**:
- Queries slow down under high load
- Timeout errors increase
- CPU stays low but throughput drops

**Causes**:
- Pool size too small for concurrent workload
- Connections not being released (leak)

**Solutions**:

Increase pool size:
```python
config = {
    "db": {
        "url": "postgresql://user:pass@localhost/db",
        "pool_size": 30  # Increased from 10
    }
}
```

### Connection Pool Resets on Restart

**Expected Behavior**: This is normal!

Each engine instance creates its own connection pool. When the engine is destroyed/recreated, connections are closed gracefully and a new pool is created.

No action needed - this is by design.

## Best Practices

### 1. Use Environment Variables

```python
import os

config = {
    "db": os.environ["DATABASE_URL"]
}
```

**Benefits**:
- Different configs per environment (dev/staging/prod)
- Secrets not in source code
- Easy deployment changes

### 2. Appropriate Pool Sizing

```python
# Small app: 10 connections (default)
# Medium app: 15-20 connections
# Large app: 25-50 connections
# Very high concurrency: 50-100+ connections

config = {
    "db": {
        "url": os.environ["DATABASE_URL"],
        "pool_size": 20  # Appropriate for medium workload
    }
}
```

### 3. Set Realistic Timeouts

```python
# Local network: 10-30 seconds
# Cloud/remote: 30-60 seconds

config = {
    "db": {
        "url": "postgresql://user:pass@remote-host/db",
        "timeout_seconds": 45  # For cloud latency
    }
}
```

### 4. Monitor Connection Health

```python
# Engine can be checked for readiness
if engine.is_ready():
    print("Engine and database connected")
else:
    print("Engine not ready, check database connection")
```

### 5. Keep Credentials Secure

**DON'T**:
```python
# ❌ Hardcode credentials
config = {
    "db": "postgresql://admin:password123@localhost/db"
}
```

**DO**:
```python
# ✅ Use environment variables
import os
config = {
    "db": os.environ["DATABASE_URL"]
}
```

### 6. Document Your Configuration

```python
# Production configuration
# - Pool size tuned for expected concurrent users
# - Timeout set for cloud latency
# - Credentials from AWS Secrets Manager via env var

config = {
    "db": {
        "url": os.environ["DATABASE_URL"],
        "pool_size": 25,      # For ~100 concurrent users
        "timeout_seconds": 45  # AWS RDS latency
    }
}
```

## Configuration Validation

FraiseQL validates configuration at engine initialization. Invalid configs will fail immediately:

```python
import json
import fraiseql

# This will raise an error
try:
    config = json.dumps({
        "db": "mysql://localhost/db"  # ❌ Invalid: MySQL not supported
    })
    engine = fraiseql.GraphQLEngine(config)
except Exception as e:
    print(f"Configuration error: {e}")
    # Output: "Invalid database URL scheme (must be postgres:// or postgresql://)"
```

### Validation Rules

| Rule | Requirement |
|------|-------------|
| **Database URL** | REQUIRED - Cannot omit `db` config |
| **URL Scheme** | MUST be `postgresql://` or `postgres://` |
| **Pool Size** | Optional, default: 10 |
| **Timeout** | Optional, default: 30 seconds |

## Reference: Complete Configuration Example

```python
import json
import os
import fraiseql

# Configuration for production environment
config = {
    # Database configuration (REQUIRED)
    "db": {
        # Connection URL from environment variable
        "url": os.environ.get(
            "DATABASE_URL",
            "postgresql://localhost/fraiseql"
        ),

        # Connection pool size (optional, default: 10)
        # Tuned for expected concurrent database users
        "pool_size": int(os.environ.get("DB_POOL_SIZE", "20")),

        # Connection acquisition timeout in seconds (optional, default: 30)
        # Increased for cloud database latency
        "timeout_seconds": int(os.environ.get("DB_TIMEOUT", "45"))
    },

    # Cache configuration (optional)
    "cache": {
        "type": "memory",
        "ttl_seconds": 3600,
        "max_size": 10000
    }
}

# Create engine with validated configuration
try:
    engine = fraiseql.GraphQLEngine(json.dumps(config))

    # Verify connection is working
    if engine.is_ready():
        print("✓ Engine initialized successfully")
        print(f"✓ Database: {os.environ.get('DATABASE_URL', 'localhost')}")
    else:
        print("✗ Engine initialized but not ready")

except Exception as e:
    print(f"✗ Failed to initialize engine: {e}")
    raise
```

## FraiseQL Architecture Notes

### Why PostgreSQL Only?

FraiseQL is **PostgreSQL-exclusive** to leverage:
- Native JSONB support (essential for GraphQL type mapping)
- Advanced query features (JSONB operators, array functions)
- Superior JSON performance
- Type-safe operations

### Pool Abstraction

FraiseQL uses an internal **pool abstraction layer** (Phase 3.1 Refactoring):

```
User Code
    ↓
GraphQLEngine (public API)
    ↓
Engine initializes ProductionPool (deadpool-postgres)
    ↓
Wraps pool as Arc<dyn PoolBackend> (trait abstraction)
    ↓
PostgresBackend storage layer uses abstraction
    ↓
Database queries executed through pool
```

**Benefits**:
- Single pool implementation (no duplication)
- Easy to swap pool types in future
- Clean separation of concerns
- Tested with real PostgreSQL connections

## Support & Issues

For connection issues:
1. Check PostgreSQL is running: `pg_isready`
2. Verify URL format matches examples above
3. Test credentials: `psql postgresql://user:pass@host/db`
4. Check logs for detailed error messages
5. See Troubleshooting section above

For bugs or feature requests, open an issue on GitHub.

## Version History

- **v1.9.5** (Jan 2026): Pool abstraction layer (Phase 3.1)
- **v1.8.3** (Dec 2025): Initial Rust pipeline
- **v1.0.0** (Early 2025): First release

---

**Last Updated**: January 8, 2026
**Framework**: FraiseQL v1.9.5
**Rust Pipeline**: Phase 3.1 (Pool Abstraction Layer)
