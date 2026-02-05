# FraiseQL v2 Troubleshooting Guide

**Last Updated:** January 26, 2026
**Version:** 2.0.0-a1

---

## Table of Contents

1. [Server Startup Issues](#server-startup-issues)
2. [Database Connection Problems](#database-connection-problems)
3. [Query Execution Errors](#query-execution-errors)
4. [Authentication & Authorization Issues](#authentication--authorization-issues)
5. [Performance Issues](#performance-issues)
6. [Logging & Debugging](#logging--debugging)
7. [Common Error Messages](#common-error-messages)
8. [Getting Help](#getting-help)

---

## Server Startup Issues

### Server Won't Start: "Address Already in Use"

**Symptom**:
```
error: Address already in use (os error 98)
```

**Diagnosis**:
```bash
# Find what's using port 8080
lsof -i :8080
ss -tulpn | grep 8080

# Check if FraiseQL process is running
ps aux | grep fraiseql
```

**Solutions**:

1. **Kill existing process** (if it's a stale process):
   ```bash
   kill -9 <PID>
   ```

2. **Use different port**:
   ```toml
   # fraiseql-server/config.toml
   [server]
   bind_addr = "0.0.0.0:8081"  # Change from 8080
   ```

3. **Check if service is running**:
   ```bash
   systemctl status fraiseql
   systemctl restart fraiseql
   ```

### Server Crashes on Startup

**Symptom**:
```
thread 'main' panicked at 'Failed to start server: ...'
```

**Possible Causes & Solutions**:

**Config File Missing**:
```bash
# Check config file path
ls -la fraiseql-server/config.toml

# Verify it's referenced correctly
echo $FRAISEQL_CONFIG
```

**Invalid TOML Syntax**:
```bash
# Validate TOML (use online validator or cargo-edit)
cargo install cargo-edit
cargo check
```

**Schema File Missing**:
```bash
# Check schema compilation
fraiseql compile schema.json -o schema.compiled.json

# Verify file exists
ls -la schema.compiled.json
```

**Invalid Environment Variables**:
```bash
# Check required variables are set
echo $DATABASE_URL
echo $JWT_SECRET

# Set missing variables
export DATABASE_URL="postgresql://user:pass@localhost/fraiseql"
export JWT_SECRET="your-secret-key-here"
```

---

## Database Connection Problems

### Cannot Connect to Database

**Symptom**:
```
error: Failed to connect to database: connection refused
error: timeout connecting to server
```

**Diagnosis**:

```bash
# Test PostgreSQL connection directly
psql "postgresql://user:pass@host:5432/database" -c "SELECT version();"

# Test MySQL connection
mysql -h host -u user -p database -e "SELECT VERSION();"

# Check if database service is running
systemctl status postgresql
systemctl status mysql
```

**Solutions**:

1. **Verify DATABASE_URL format**:
   ```bash
   # PostgreSQL
   postgresql://username:password@hostname:5432/database_name

   # MySQL
   mysql://username:password@hostname:3306/database_name

   # SQLite
   sqlite://./fraiseql.db

   # SQL Server
   sqlserver://username:password@hostname:1433/database_name
   ```

2. **Check database credentials**:
   ```bash
   # Test with psql
   psql -h localhost -U fraiseql_user -d fraiseql -c "SELECT 1;"
   ```

3. **Verify network connectivity**:
   ```bash
   # Test network reachability
   ping database-host
   telnet database-host 5432
   nc -zv database-host 5432
   ```

4. **Check firewall rules**:
   ```bash
   # On database server
   sudo ufw status
   sudo ufw allow 5432/tcp  # For PostgreSQL
   ```

5. **Verify database exists**:
   ```bash
   # PostgreSQL
   psql -h localhost -U postgres -l | grep fraiseql

   # MySQL
   mysql -h localhost -u root -p -e "SHOW DATABASES;"
   ```

### Connection Pool Exhausted

**Symptom**:
```
error: No connections available in pool
error: Timeout waiting for connection from pool
```

**Diagnosis**:

```bash
# Check number of active connections (PostgreSQL)
psql $DATABASE_URL -c "SELECT count(*) FROM pg_stat_activity;"

# Check pool size configuration
grep connection_pool_size fraiseql-server/config.toml
```

**Solutions**:

1. **Increase pool size** in config:
   ```toml
   [performance]
   connection_pool_size = 50  # Increase from default 20
   ```

2. **Reduce idle timeout** to free connections:
   ```toml
   [performance]
   connection_idle_timeout_secs = 30  # Reduce from 60
   ```

3. **Check for connection leaks**:
   ```bash
   # Monitor connections over time
   watch -n 5 "psql $DATABASE_URL -c 'SELECT count(*) FROM pg_stat_activity;'"
   ```

4. **Restart connection pool**:
   ```bash
   # Restart the server to reset connections
   systemctl restart fraiseql
   ```

---

## Query Execution Errors

### Query Depth Exceeded Limit

**Symptom**:
```json
{
  "errors": [{
    "message": "Query depth exceeded limit (max: 10)"
  }]
}
```

**Cause**: Query has too many levels of nested fields.

**Example**:
```graphql
# This query has depth 4 (users → posts → comments → author)
{
  users {
    id
    posts {
      id
      comments {
        id
        author {
          id
        }
      }
    }
  }
}
```

**Solutions**:

1. **Reduce query depth** - split into multiple queries:
   ```graphql
   # Query 1: Get users and posts
   {
    users {
      id
      posts {
        id
      }
    }
   }

   # Query 2: Get comments separately
   {
    posts(id: $postId) {
      comments {
        author {
          id
        }
      }
    }
   }
   ```

2. **Increase limit** if needed:
   ```toml
   [security]
   query_max_depth = 15  # Increase from 10
   ```

### Query Complexity Exceeded Limit

**Symptom**:
```json
{
  "errors": [{
    "message": "Query complexity exceeded limit (max: 1000)"
  }]
}
```

**Cause**: Query is too expensive to execute (many list fields or nested lists).

**Complexity Scoring**:

- Scalar field = 1 point
- Object field = 1 point
- List field = 5 points (multiplied by nested list counts)

**Example**:
```graphql
# Complexity: (1 + 1 + 5 * (1 + 5 * 1)) = 32 points
{
  users {                 # 1
    id                    # 1
    posts {               # 5 (list)
      id                  # 1
      comments {          # 5 (list)
        id                # 1
      }
    }
  }
}
```

**Solutions**:

1. **Add pagination** to list fields:
   ```graphql
   {
    users {
      id
      posts(first: 10) {      # Add limit
        id
        comments(first: 5) {  # Add limit
          id
        }
      }
    }
   }
   ```

2. **Use field selection** to reduce complexity:
   ```graphql
   # Only request needed fields
   {
    users {
      id
      name
      # Skip posts.comments.author
      posts(first: 5) {
        id
        title
      }
    }
   }
   ```

3. **Increase limit** if necessary:
   ```toml
   [security]
   query_max_complexity = 2000  # Increase from 1000
   ```

### Query Timeout

**Symptom**:
```json
{
  "errors": [{
    "message": "Query execution timeout (exceeded 30000ms)"
  }]
}
```

**Cause**: Query took longer than configured timeout.

**Solutions**:

1. **Add database indexes** on frequently-filtered columns:
   ```sql
   -- PostgreSQL
   CREATE INDEX idx_users_status ON users(status);
   CREATE INDEX idx_posts_user_id ON posts(user_id);
   ```

2. **Reduce query scope**:
   ```graphql
   # Instead of fetching all users
   {
    allUsers {
      id
      name
    }
   }

   # Use filtering and pagination
   {
    users(status: "active", first: 20) {
      id
      name
    }
   }
   ```

3. **Increase timeout** (use cautiously):
   ```toml
   [performance]
   query_timeout_ms = 60000  # Increase from 30000
   ```

4. **Analyze slow queries**:
   ```bash
   # PostgreSQL
   export RUST_LOG="fraiseql_core::executor=debug"
   fraiseql-server -c config.toml

   # Check database stats
   psql $DATABASE_URL -c "
     SELECT query, calls, mean_exec_time
     FROM pg_stat_statements
     ORDER BY mean_exec_time DESC
     LIMIT 10;
   "
   ```

### Parse Error

**Symptom**:
```json
{
  "errors": [{
    "message": "Failed to parse query: Syntax error at position 42"
  }]
}
```

**Diagnosis**:

1. **Validate GraphQL syntax** (use online validator):
   - GraphQL Playground: http://localhost:8080/playground
   - Validate missing braces, colons, etc.

2. **Check field names** are correct in schema

3. **Verify variable types**:
   ```graphql
   # ❌ Wrong - missing variable definition
   {
    users(limit: $limit) { id }
   }

   # ✅ Correct
   query GetUsers($limit: Int!) {
    users(limit: $limit) { id }
   }
   ```

### Validation Error

**Symptom**:
```json
{
  "errors": [{
    "message": "Field 'unknownField' does not exist on type 'User'"
  }]
}
```

**Solutions**:

1. **Check field name** in schema matches exactly (case-sensitive)

2. **Check field is not restricted** by authorization:
   ```json
   {
     "errors": [{
       "message": "Not authorized to access field 'User.ssn'"
     }]
   }
   ```
   Solution: Check authorization rules in SECURITY.md or contact admin.

3. **Verify token is valid** if getting auth errors:
   ```bash
   # Check JWT token
   jwt.io  # Paste token to validate
   ```

---

## Authentication & Authorization Issues

### Invalid Token Error

**Symptom**:
```json
{
  "errors": [{
    "message": "Invalid or expired token"
  }]
}
```

**Diagnosis**:

```bash
# Check if token is set
curl -H "Authorization: Bearer $JWT_TOKEN" http://localhost:8080/graphql

# Decode token (check expiration)
# Use jwt.io or:
echo $JWT_TOKEN | cut -d '.' -f 2 | base64 -d | jq .
```

**Solutions**:

1. **Refresh token** if expired:
   ```bash
   # Request new token from auth provider
   # Then include in subsequent requests
   curl -H "Authorization: Bearer $NEW_TOKEN" ...
   ```

2. **Verify JWT_SECRET** matches what signed the token:
   ```bash
   # Token signed with one secret won't validate with another
   export JWT_SECRET="correct-secret-key"
   ```

3. **Check token algorithm** matches config:
   ```toml
   [auth]
   jwt_algorithms = ["RS256", "HS256"]  # Token must use one of these
   ```

4. **Check token is in Authorization header** with "Bearer " prefix:
   ```bash
   # Correct format
   curl -H "Authorization: Bearer eyJhbGc..."

   # Incorrect (missing Bearer)
   curl -H "Authorization: eyJhbGc..."
   ```

### Missing Authorization Header

**Symptom**:
```json
{
  "errors": [{
    "message": "Missing authorization header"
  }]
}
```

**Solution**: Add authorization header to request:
```bash
curl -H "Authorization: Bearer $JWT_TOKEN" http://localhost:8080/graphql
```

### Access Denied (Insufficient Permissions)

**Symptom**:
```json
{
  "errors": [{
    "message": "Not authorized to access field 'User.salary'",
    "extensions": {
      "reason": "Missing role: admin"
    }
  }]
}
```

**Diagnosis**:

1. **Check user roles in token**:
   ```bash
   echo $JWT_TOKEN | cut -d '.' -f 2 | base64 -d | jq .roles
   ```

2. **Check field authorization rules** in schema:
   ```graphql
   type User {
     salary: Int! @authorize(roles: ["admin"])
   }
   ```

3. **Check custom authorization rules**:
   ```graphql
   type User {
     email: String! @authorize(
       update: "isOwner($user, $field.ownerId)"
     )
   }
   ```

**Solutions**:

1. **Request higher privilege role** from auth provider

2. **Use different field** if available and not restricted

3. **Contact admin** to update authorization rules if needed

---

## Performance Issues

### High Memory Usage

**Symptom**:

- Server process consuming > 1GB RAM
- System becoming unresponsive
- OOM killer terminating process

**Diagnosis**:

```bash
# Check memory usage
ps aux | grep fraiseql

# Monitor memory over time
watch -n 1 "ps aux | grep fraiseql"

# Check cache settings
grep cache fraiseql-server/config.toml
grep connection_pool_size fraiseql-server/config.toml
```

**Solutions**:

1. **Reduce query cache size**:
   ```toml
   [performance]
   cache_max_size = 5000  # Reduce from 10000
   cache_ttl_secs = 300   # Reduce from 600
   ```

2. **Reduce connection pool size**:
   ```toml
   [performance]
   connection_pool_size = 10  # Reduce from 20
   ```

3. **Disable caching** if not needed:
   ```toml
   [performance]
   cache_enabled = false
   ```

4. **Monitor with profiler**:
   ```bash
   # Use heaptrack or flamegraph
   heaptrack fraiseql-server -c config.toml
   ```

### Slow Queries

**Symptom**:

- Queries taking > 5 seconds
- P99 latency > 30 seconds
- Timeout errors during high load

**Diagnosis**:

```bash
# Enable debug logging
export RUST_LOG="fraiseql_core::executor=debug"
fraiseql-server -c config.toml

# Check database query stats (PostgreSQL)
psql $DATABASE_URL -c "
  SELECT query, calls, mean_exec_time, max_exec_time
  FROM pg_stat_statements
  ORDER BY mean_exec_time DESC
  LIMIT 20;
"
```

**Solutions**:

1. **Create database indexes**:
   ```sql
   -- Find missing indexes
   psql $DATABASE_URL -c "
     SELECT schemaname, tablename
     FROM pg_tables
     WHERE schemaname NOT IN ('pg_catalog', 'information_schema');
   "

   -- Create indexes on frequently-filtered columns
   CREATE INDEX idx_name ON table_name(column_name);
   ```

2. **Add query pagination**:
   ```graphql
   # Fetch smaller batches
   {
    users(first: 20, after: $cursor) {
      id
      name
    }
   }
   ```

3. **Check query plan** (PostgreSQL):
   ```sql
   EXPLAIN (FORMAT JSON, ANALYZE)
   SELECT * FROM users WHERE status = 'active' LIMIT 20;
   ```

4. **Increase connection pool** if waiting for connections:
   ```toml
   [performance]
   connection_pool_size = 50
   ```

### High CPU Usage

**Symptom**:

- CPU at 100% during normal load
- Server not responding to requests

**Diagnosis**:

```bash
# Check if CPU-bound or I/O-bound
iostat -x 1 10  # Check disk I/O
top -b -n 1     # Check CPU usage

# Profile CPU usage
perf top -p $(pgrep fraiseql-server)
```

**Solutions**:

1. **Reduce query complexity** (see Query Complexity Errors section)

2. **Enable query caching**:
   ```toml
   [performance]
   cache_enabled = true
   apq_enabled = true  # Automatic Persistent Queries
   ```

3. **Scale horizontally** - add more server instances

4. **Profile with flamegraph**:
   ```bash
   cargo flamegraph --bin fraiseql-server
   # Look for hot functions in flamegraph.svg
   ```

---

## Logging & Debugging

### Enable Debug Logging

**Set log level**:
```bash
# Development (verbose)
export RUST_LOG="fraiseql=debug"

# Production (info level)
export RUST_LOG="fraiseql=info"

# Specific module
export RUST_LOG="fraiseql_core::executor=debug,fraiseql_server=info"
```

**View logs**:
```bash
# STDOUT (when running locally)
fraiseql-server -c config.toml

# From Docker
docker logs fraiseql-container

# From systemd
journalctl -u fraiseql -f

# From file (if configured)
tail -f /var/log/fraiseql/fraiseql.log
```

### Health Check Endpoint

**Test server health**:
```bash
curl http://localhost:8080/health

# Response:
{
  "status": "healthy",
  "database": "connected",
  "uptime_seconds": 3600,
  "metrics": {
    "queries_total": 1234,
    "errors_total": 2,
    "cache_hit_rate": 0.87
  }
}
```

### Metrics Endpoint

**Collect Prometheus metrics**:
```bash
curl http://localhost:8080/metrics

# Output (Prometheus format):
# fraiseql_query_duration_ms{query="getUserById"} 45
# fraiseql_cache_hit_rate 0.87
# fraiseql_db_pool_active_connections 12
```

### Distributed Tracing

**Enable OpenTelemetry tracing**:
```bash
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4317"
fraiseql-server -c config.toml
```

**Query Jaeger UI**:
```bash
# Access at http://localhost:16686
# Filter by service: fraiseql-server
# View trace details for individual queries
```

---

## Common Error Messages

### "Failed to load compiled schema"

**Cause**: `schema.compiled.json` not found or invalid

**Solution**:
```bash
# Recompile schema
fraiseql compile schema.json -o schema.compiled.json

# Verify file exists and is valid JSON
cat schema.compiled.json | jq . > /dev/null
```

### "Connection pool size must be > 0"

**Cause**: Config has invalid pool size

**Solution**:
```toml
[performance]
connection_pool_size = 20  # Must be >= 1
```

### "CORS origin not allowed"

**Cause**: Request origin not in configured CORS origins

**Solution** (see DEPLOYMENT.md CORS Configuration):
```toml
[features]
cors_origins = ["https://app.example.com"]  # Add your origin
```

### "Rate limit exceeded"

**Symptom**: `429 Too Many Requests`

**Cause**: Too many requests from same IP in time window

**Solution**:
```toml
[security]
rate_limit_enabled = true
rate_limit_requests = 1000      # Increase if needed
rate_limit_window_secs = 60
```

### "Query depth exceeded" / "Query complexity exceeded"

**See**: [Query Execution Errors](#query-execution-errors) section

### "Introspection is disabled"

**Symptom**: Cannot introspect schema for client tooling

**Solution** (if needed for development):
```toml
[security]
introspection_enabled = true  # Only in development!
```

---

## Getting Help

### Check Logs for Error Details

1. **Enable debug logging** (see [Logging & Debugging](#logging--debugging))
2. **Look for error context** in structured logs
3. **Note the trace ID** for correlating errors

### Common Resources

- **Documentation**: See [README.md](README.md)
- **Security Model**: See [SECURITY.md](SECURITY.md)
- **Deployment Guide**: See [DEPLOYMENT.md](DEPLOYMENT.md)
- **Issues/Bugs**: Report at https://github.com/fraiseql/fraiseql/issues
- **Security Issues**: Email security@fraiseql.dev

### Before Reporting Issues

Provide:

1. **Exact error message** (full stack trace if available)
2. **Minimal reproducible example** (GraphQL query that fails)
3. **Configuration** (sanitized config.toml, no secrets)
4. **Environment** (OS, database version, FraiseQL version)
5. **Logs** (with RUST_LOG=debug enabled)

### System Information Command

```bash
# Gather diagnostic info
echo "=== FraiseQL Version ==="
fraiseql --version

echo "=== Rust Version ==="
rustc --version

echo "=== Database Version ==="
psql --version  # or mysql --version

echo "=== Server Status ==="
curl http://localhost:8080/health | jq .

echo "=== Running Processes ==="
ps aux | grep fraiseql

echo "=== Recent Logs ==="
RUST_LOG=debug fraiseql-server -c config.toml 2>&1 | tail -50
```

---

**Remember**: Most issues can be resolved by:

1. Checking logs with appropriate log level
2. Verifying configuration is correct
3. Testing connectivity to database
4. Ensuring authentication/authorization tokens are valid
