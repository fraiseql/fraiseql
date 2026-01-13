# Troubleshooting Guide for fraiseql-wire

This guide covers common issues you may encounter when using fraiseql-wire and how to resolve them.

---

## Table of Contents

1. [Connection Errors](#connection-errors)
2. [Authentication Issues](#authentication-issues)
3. [Query Errors](#query-errors)
4. [Schema & Data Issues](#schema--data-issues)
5. [Performance Issues](#performance-issues)
6. [Network Issues](#network-issues)
7. [Error Messages Reference](#error-messages-reference)

---

## Connection Errors

### Error: "connection refused"

**Symptoms**:
```
Error: connection error: failed to connect to localhost:5432: connection refused.
Is Postgres running?
```

**Causes**:
1. Postgres is not running
2. Postgres is not listening on the specified address/port
3. Firewall blocking the connection
4. Wrong host or port specified

**Solutions**:

1. **Verify Postgres is running**:
   ```bash
   pg_isready -h localhost -p 5432
   # Should output: accepting connections
   ```

2. **Start Postgres if not running**:
   ```bash
   # Linux/systemd
   sudo systemctl start postgresql

   # macOS/Homebrew
   brew services start postgresql

   # Docker
   docker run -d --name postgres -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:17
   ```

3. **Verify connection string**:
   ```rust
   // TCP (must be running and listening)
   FraiseClient::connect("postgres://localhost:5432/mydb").await?;

   // Unix socket (requires Postgres on same machine)
   FraiseClient::connect("postgres:///mydb").await?;
   ```

4. **Check if port is in use**:
   ```bash
   lsof -i :5432  # See what's using port 5432
   netstat -tlnp | grep 5432  # Alternative on Linux
   ```

5. **Firewall check**:
   ```bash
   # Try connecting directly with psql
   psql -h localhost -p 5432 -U postgres -d postgres
   ```

---

### Error: "connection closed"

**Symptoms**:
```
Error: connection error: connection closed unexpectedly
```

**Causes**:
1. Postgres restarted mid-stream
2. Network disconnection
3. Postgres ran out of memory
4. Connection idle timeout

**Solutions**:

1. **Check Postgres logs**:
   ```bash
   # Find log file
   sudo -u postgres psql -c "SHOW log_directory;"
   tail -f /var/log/postgresql/postgresql.log
   ```

2. **Verify Postgres is still running**:
   ```bash
   pg_isready
   systemctl status postgresql
   ```

3. **Check network connectivity**:
   ```bash
   ping postgres-host
   telnet postgres-host 5432
   ```

4. **Increase idle timeout** (add to connection parameters):
   ```rust
   let client = FraiseClient::connect(
       "postgres://user:pass@localhost/db?application_name=fraiseql"
   ).await?;
   ```

5. **Implement retry logic**:
   ```rust
   async fn query_with_retry(
       conn_string: &str,
       table: &str,
       max_retries: u32,
   ) -> Result<Box<dyn Stream<Item = Result<Value>> + Unpin>> {
       let mut retries = 0;
       loop {
           match FraiseClient::connect(conn_string).await {
               Ok(client) => return client.query(table).execute().await,
               Err(e) if retries < max_retries => {
                   eprintln!("Retry {} after error: {}", retries, e);
                   retries += 1;
                   tokio::time::sleep(Duration::from_secs(1)).await;
               }
               Err(e) => return Err(e),
           }
       }
   }
   ```

---

## Authentication Issues

### Error: "authentication failed: invalid password"

**Symptoms**:
```
Error: authentication failed: invalid password. Check credentials.
```

**Causes**:
1. Wrong password
2. Wrong username
3. User doesn't have login privilege
4. User doesn't have access to the database

**Solutions**:

1. **Verify credentials with psql**:
   ```bash
   psql -U myuser -W -h localhost -d mydb
   # Enter password when prompted
   ```

2. **Check user exists**:
   ```bash
   sudo -u postgres psql -c "\du myuser"
   # Should show user in list
   ```

3. **Reset password** (if you're a Postgres admin):
   ```bash
   sudo -u postgres psql -c "ALTER USER myuser WITH PASSWORD 'newpassword';"
   ```

4. **Check user has login privilege**:
   ```bash
   sudo -u postgres psql -c "\du myuser"
   # Output should show: Login | Superuser | etc.
   # If Login shows "|" instead of "yes", user can't login
   ```

5. **Grant login privilege**:
   ```bash
   sudo -u postgres psql -c "ALTER USER myuser WITH LOGIN;"
   ```

6. **Check user has database access**:
   ```bash
   sudo -u postgres psql -c "GRANT CONNECT ON DATABASE mydb TO myuser;"
   sudo -u postgres psql -c "GRANT USAGE ON SCHEMA public TO myuser;"
   ```

7. **Use correct connection string format**:
   ```rust
   // Format: postgres://user:password@host:port/database
   // Password with special characters must be URL-encoded
   let pwd = "pass@word!";  // contains @
   let encoded = urlencoding::encode(pwd);
   let conn = format!("postgres://user:{}@host/db", encoded);
   ```

---

### Error: "authentication failed: role does not exist"

**Symptoms**:
```
Error: authentication failed: role "myuser" does not exist
```

**Solutions**:

1. **Create the user**:
   ```bash
   sudo -u postgres createuser myuser
   sudo -u postgres psql -c "ALTER USER myuser WITH PASSWORD 'password';"
   ```

2. **Check available users**:
   ```bash
   sudo -u postgres psql -c "\du"
   ```

---

## Query Errors

### Error: "invalid result schema"

**Symptoms**:
```
Error: invalid result schema: query returned 2 columns instead of 1.
fraiseql-wire supports only SELECT data queries.
```

**Causes**:
1. Query returns multiple columns (fraiseql-wire only supports single column)
2. Column is not named `data`
3. Column is not JSON/JSONB type

**Solutions**:

1. **Ensure column is named `data`**:
   ```sql
   -- ✅ CORRECT
   SELECT data FROM v_projects;

   -- ❌ WRONG
   SELECT *, data FROM v_projects;  -- Multiple columns
   SELECT id, data FROM v_projects;  -- Multiple columns
   SELECT data AS project FROM v_projects;  -- Wrong column name
   ```

2. **Check view/table structure**:
   ```bash
   psql -d mydb -c "\d v_projects"
   # Should show: data | jsonb
   ```

3. **Create proper view if needed**:
   ```sql
   -- ✅ CORRECT - Single JSON column
   CREATE VIEW v_my_entity AS
   SELECT data FROM my_entity_table;
   ```

4. **Verify column type is JSON**:
   ```bash
   psql -d mydb -c "
   SELECT column_name, data_type
   FROM information_schema.columns
   WHERE table_name = 'v_projects' AND column_name = 'data';"
   ```

---

### Error: "sql error: relation does not exist"

**Symptoms**:
```
Error: sql error: relation "v_projects" does not exist
```

**Causes**:
1. View/table doesn't exist
2. View/table is in different schema
3. Wrong table name

**Solutions**:

1. **Check table exists**:
   ```bash
   psql -d mydb -c "\dv v_projects"
   # Or for tables:
   psql -d mydb -c "\d v_projects"
   ```

2. **List all available views**:
   ```bash
   psql -d mydb -c "\dv"
   ```

3. **Create missing view**:
   ```sql
   CREATE VIEW v_projects AS
   SELECT id, data FROM projects;
   ```

4. **If view is in schema, use full name**:
   ```rust
   // If view is in test_staging schema:
   client.query("test_staging.v_projects").execute().await?;
   ```

5. **Check schema exists**:
   ```bash
   psql -d mydb -c "\dn"  # List schemas
   ```

---

### Error: "sql error: column does not exist"

**Symptoms**:
```
Error: sql error: column "project__status__name" does not exist
```

**Causes**:
1. WHERE clause references non-existent column/JSON key
2. Wrong JSON path syntax
4. Data doesn't have the expected structure

**Solutions**:

1. **Use correct JSON path syntax**:
   ```rust
   // ✅ CORRECT - JSON key access
   .where_sql("data->>'status' = 'active'")

   // ❌ WRONG - Direct column reference
   .where_sql("status = 'active'")  // 'status' is inside JSON!
   ```

2. **Check actual JSON structure**:
   ```bash
   psql -d mydb -c "
   SELECT jsonb_pretty(data) FROM v_projects LIMIT 1;"
   ```

3. **Use Rust predicates for complex filtering**:
   ```rust
   client
       .query("projects")
       .where_rust(|json| {
           json.get("status")
               .and_then(|s| s.as_str())
               .map(|s| s == "active")
               .unwrap_or(false)
       })
       .execute()
       .await?
   ```

4. **Test SQL predicate in psql first**:
   ```bash
   psql -d mydb -c "SELECT data FROM v_projects WHERE data->>'status' = 'active';"
   ```

---

### Error: "sql error: syntax error"

**Symptoms**:
```
Error: sql error: syntax error at or near "WHERE"
```

**Causes**:
1. Malformed WHERE clause
2. Unescaped quotes in predicate
3. Invalid SQL syntax

**Solutions**:

1. **Test WHERE clause directly**:
   ```bash
   psql -d mydb -c "
   SELECT data FROM v_projects
   WHERE data->>'name' = 'Alpha';"
   ```

2. **Escape quotes properly in Rust**:
   ```rust
   // ✅ CORRECT - Using single quotes for SQL strings
   .where_sql("data->>'name' = 'Alpha'")

   // ❌ WRONG - Unescaped quotes
   .where_sql("data->>'name' = \"Alpha\"")  // Will break!

   // For quotes in string:
   .where_sql("data->>'name' = 'O''Brien'")  // Escape with ''
   ```

3. **Use parameterized queries (if supported)**:
   ```rust
   // If searching user input, be careful:
   let name = "O'Brien";
   let safe_predicate = format!(
       "data->>'name' = '{}'",
       name.replace("'", "''")  // Escape quotes
   );
   ```

---

## Schema & Data Issues

### Error: "invalid json" or JSON decode errors

**Symptoms**:
```
Error: json decode error: invalid type: map, expected a sequence at line 1 column 0
```

**Causes**:
1. Data in `data` column isn't valid JSON
2. Data structure doesn't match what you're trying to deserialize
3. Corruption in database

**Solutions**:

1. **Verify JSON is valid**:
   ```bash
   psql -d mydb -c "
   SELECT data FROM v_projects WHERE data IS NOT NULL LIMIT 1;"
   ```

2. **Check with psql's JSON functions**:
   ```bash
   psql -d mydb -c "
   SELECT jsonb_valid(data) FROM v_projects LIMIT 10;"
   # Should all be 't' (true)
   ```

3. **Fix invalid JSON** (if you can identify it):
   ```bash
   psql -d mydb -c "
   DELETE FROM projects WHERE NOT jsonb_valid(data);"
   ```

4. **Inspect actual structure**:
   ```rust
   let mut stream = client.query("projects").execute().await?;
   while let Some(result) = stream.next().await {
       match result {
           Ok(value) => {
               println!("Type: {}", value.type_str());
               println!("Value: {}", value);
           }
           Err(e) => eprintln!("Error: {}", e),
       }
   }
   ```

---

### Error: Empty result sets when expecting data

**Symptoms**:
- Query executes without error but returns 0 rows
- WHERE clause filters out all data

**Solutions**:

1. **Verify data exists**:
   ```bash
   psql -d mydb -c "SELECT COUNT(*) FROM v_projects;"
   ```

2. **Test WHERE clause**:
   ```bash
   psql -d mydb -c "
   SELECT COUNT(*) FROM v_projects
   WHERE data->>'status' = 'active';"
   ```

3. **Check JSON structure matches predicate**:
   ```bash
   psql -d mydb -c "
   SELECT jsonb_pretty(data) FROM v_projects LIMIT 1;"
   ```

4. **Relax predicates for debugging**:
   ```rust
   // Remove WHERE clause temporarily
   let mut stream = client.query("projects").execute().await?;
   let count = stream.count().await;  // How many rows total?
   ```

---

## Performance Issues

### Symptom: "Throughput is lower than expected"

**Possible Causes**:
1. Network latency
2. Chunk size not optimized
3. WHERE clause not filtering on server
4. Large JSON objects
5. Slow Postgres query

**Solutions**:

1. **Increase chunk_size** (reduces latency overhead):
   ```rust
   client
       .query("projects")
       .chunk_size(512)  // Default is 256, try 512
       .execute()
       .await?
   ```

2. **Move filtering to SQL** (reduce network transfer):
   ```rust
   // ❌ SLOW - Get all rows, filter client-side
   client
       .query("projects")
       .where_rust(|json| {
           json.get("status").as_str() == Some("active")
       })
       .execute()
       .await?

   // ✅ FAST - Filter on server
   client
       .query("projects")
       .where_sql("data->>'status' = 'active'")
       .execute()
       .await?
   ```

3. **Check network latency**:
   ```bash
   ping postgres-host
   # Add time for round-trip
   ```

4. **Verify Postgres isn't slow**:
   ```bash
   psql -d mydb -c "
   EXPLAIN ANALYZE
   SELECT data FROM v_projects WHERE data->>'status' = 'active';"
   ```

5. **Add index to accelerate queries**:
   ```sql
   CREATE INDEX idx_projects_status ON projects
   USING GIN (data);
   ```

6. **See PERFORMANCE_TUNING.md** for detailed optimization strategies.

---

### Symptom: "Memory usage is very high"

**Possible Causes**:
1. chunk_size is too large
2. Large JSON objects (100KB+)
3. Getting many rows at once
4. Rust predicates materializing data

**Solutions**:

1. **Reduce chunk_size**:
   ```rust
   client
       .query("projects")
       .chunk_size(64)  // Smaller = less memory per batch
       .execute()
       .await?
   ```

2. **Filter with WHERE clause** (reduces data):
   ```rust
   client
       .query("projects")
       .where_sql("data->>'status' = 'active'")
       .chunk_size(256)
       .execute()
       .await?
   ```

3. **Monitor actual usage**:
   ```bash
   # In separate terminal
   watch -n1 'ps aux | grep fraiseql'
   ```

4. **Remember fraiseql-wire is O(chunk_size), not O(result_size)**:
   - If you have 10M rows but chunk_size = 256
   - Memory usage is bound to ~a few MB, not GB

---

## Network Issues

### Error: "timeout" or "operation timed out"

**Symptoms**:
```
Error: io error: operation timed out
```

**Causes**:
1. Network latency too high
2. Postgres is slow responding
3. Query takes very long to execute
4. Firewall dropping idle connections

**Solutions**:

1. **Check network latency**:
   ```bash
   ping -c 10 postgres-host
   # If RTT > 100ms, consider using Unix socket
   ```

2. **Use Unix socket** (if Postgres on same machine):
   ```rust
   // TCP (slower, network overhead)
   FraiseClient::connect("postgres://localhost:5432/db").await?;

   // Unix socket (faster, no network stack)
   FraiseClient::connect("postgres:///db").await?;
   ```

3. **Optimize Postgres query** (see PERFORMANCE_TUNING.md):
   ```bash
   psql -d mydb -c "
   EXPLAIN ANALYZE
   SELECT data FROM v_projects WHERE ...;"
   ```

4. **Implement query timeout** (if in future versions):
   ```rust
   // For now, use tokio::time::timeout:
   let result = tokio::time::timeout(
       Duration::from_secs(30),
       client.query("projects").execute()
   ).await;
   ```

---

### Error: "connection reset by peer"

**Symptoms**:
```
Error: io error: connection reset by peer
```

**Causes**:
1. Postgres crashed
2. Network connectivity lost
3. Firewall/proxy closing connection
4. Postgres restarted

**Solutions**:

1. **Check if Postgres is running**:
   ```bash
   pg_isready
   ```

2. **Check network connectivity**:
   ```bash
   ping postgres-host
   traceroute postgres-host
   ```

3. **Check firewall/proxy**:
   ```bash
   # Try direct connection
   nc -zv postgres-host 5432
   ```

4. **Implement reconnection logic**:
   ```rust
   async fn robust_query(
       conn_string: &str,
       table: &str,
   ) -> Result<Box<dyn Stream<Item = Result<Value>> + Unpin>> {
       match FraiseClient::connect(conn_string).await {
           Ok(client) => client.query(table).execute().await,
           Err(_) => {
               // Wait and retry
               tokio::time::sleep(Duration::from_secs(5)).await;
               FraiseClient::connect(conn_string)
                   .await?
                   .query(table)
                   .execute()
                   .await
           }
       }
   }
   ```

---

## Error Messages Reference

### Connection Category
| Error | Meaning | Fix |
|-------|---------|-----|
| "connection refused" | Postgres not running/listening | Start Postgres, check host/port |
| "connection closed" | Unexpected disconnect | Check Postgres logs, verify connectivity |
| "invalid connection string" | Malformed connection URL | Check `postgres://user:pass@host/db` format |
| "connection already in use" | Trying to query twice on one connection | Create new client or wait for stream to complete |

### Authentication Category
| Error | Meaning | Fix |
|-------|---------|-----|
| "invalid password" | Wrong credentials | Verify user/password with `psql` |
| "role does not exist" | User doesn't exist | Create user: `createuser username` |
| "permission denied" | User lacks privileges | Grant privileges: `GRANT CONNECT ON DATABASE...` |

### Query Category
| Error | Meaning | Fix |
|-------|---------|-----|
| "invalid result schema" | Query doesn't return single JSON column | Use `SELECT data FROM ...` |
| "relation does not exist" | Table/view missing | Create view or check table name |
| "column does not exist" | WHERE clause uses wrong column | Use `data->>'key'` for JSON access |
| "syntax error" | Malformed SQL | Test WHERE clause in `psql` |

### Data Category
| Error | Meaning | Fix |
|-------|---------|-----|
| "json decode error" | Invalid JSON in `data` column | Check data validity with `jsonb_valid()` |
| "invalid json" | JSON parsing failed | Inspect with `jsonb_pretty()` |
| "unexpected type" | JSON structure doesn't match | Check actual JSON with `psql` |

---

## Getting Help

### Check the Documentation

- **Performance Tuning**: `PERFORMANCE_TUNING.md`
- **Security**: `SECURITY.md`
- **Testing**: `TESTING_GUIDE.md`
- **API Docs**: `cargo doc --open`

### Debug Mode

Enable detailed logging:

```rust
use tracing::info;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Enable debug logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Starting fraiseql-wire client");

    let client = FraiseClient::connect("postgres://localhost/db").await?;
    // ... rest of code

    Ok(())
}
```

### Common Patterns

**Safe predicate handling**:
```rust
fn escape_sql_string(s: &str) -> String {
    s.replace("'", "''")
}

let status = "O'Brien";
let safe = escape_sql_string(status);
client
    .query("users")
    .where_sql(&format!("data->>'name' = '{}'", safe))
    .execute()
    .await?
```

**Robust streaming**:
```rust
match client.query("projects").execute().await {
    Ok(mut stream) => {
        while let Some(result) = stream.next().await {
            match result {
                Ok(value) => println!("{}", value),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
    Err(e) => eprintln!("Query failed: {}", e),
}
```

---

**Still stuck?** Check the error message carefully - it should guide you to the solution. Common issues are almost always:
1. Postgres not running
2. Wrong connection string
3. Query returning wrong column structure
4. WHERE clause using wrong JSON syntax

See `SECURITY.md` and `PERFORMANCE_TUNING.md` for more detailed guidance.
