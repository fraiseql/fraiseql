# Security Guide for fraiseql-wire

This document provides security best practices for using fraiseql-wire in your application.

---

## Table of Contents

1. [Deployment Security](#deployment-security)
2. [Query Security](#query-security)
3. [Credential Management](#credential-management)
4. [Known Limitations](#known-limitations)
5. [Reporting Security Issues](#reporting-security-issues)

---

## Deployment Security

### Transport Security

#### For Local Development (Single Machine)

**Recommended**: Use Unix socket (default when connecting to `localhost`):

```rust
// ✅ SAFE for local development
let client = FraiseClient::connect("postgres:///mydb").await?;
```

**Why**: Unix sockets use filesystem permissions for authentication, no network exposure.

#### For Production Deployments

**Current Status** (v0.1.0): TLS support not yet implemented.

**Temporary Workaround**:
1. **Use Unix sockets** if Postgres and application are on the same machine
2. **VPN or private network** to Postgres server
3. **SSH tunnel** to Postgres:
   ```bash
   ssh -N -L 5432:localhost:5432 user@remote-host
   ```

**Permanent Solution** (Phase 8):
- TLS support will be added in future release
- Watch ROADMAP.md for implementation status

⚠️ **Do NOT** use fraiseql-wire with TCP connections in production until TLS is implemented.

---

## Query Security

### SQL Injection Prevention

fraiseql-wire uses the Postgres **Simple Query protocol**, which does not support parameterized queries. This means you must take care when constructing WHERE and ORDER BY clauses.

#### Safe Patterns

**Pattern 1: Hardcoded Predicates** ✅ SAFE

```rust
client.query("users")
    .where_sql("data->>'status' = 'active'")
    .execute()
    .await?
```

Why: Static string, no user input.

**Pattern 2: Rust-Side Predicates** ✅ SAFE

```rust
client.query("users")
    .where_rust(|json| {
        json["status"].as_str() == Some("active")
    })
    .execute()
    .await?
```

Why: Rust handles type checking, no SQL injection possible.

**Pattern 3: Whitelist Validation** ✅ SAFE

```rust
let valid_statuses = ["active", "inactive", "pending"];
if !valid_statuses.contains(&request.status) {
    return Err("Invalid status");
}

client.query("users")
    .where_sql(&format!("data->>'status' = '{}'", request.status))
    .execute()
    .await?
```

Why: User input validated against whitelist before SQL generation.

#### Unsafe Patterns

**Pattern 1: Direct Interpolation** ❌ UNSAFE

```rust
// Never do this!
let user_input = get_user_input();  // Could be: "active' OR '1'='1"
client.query("users")
    .where_sql(&format!("data->>'status' = '{}'", user_input))
    .execute()
    .await?
```

Why: User input could contain SQL operators.

**Pattern 2: Unchecked URL Parameters** ❌ UNSAFE

```rust
// Never do this!
let status = req.query("status")?;  // Untrusted HTTP input
client.query("users")
    .where_sql(&format!("data->>'status' = '{}'", status))
    .execute()
    .await?
```

Why: HTTP input is untrusted and could be malicious.

### Best Practices

1. **Prefer `.where_rust()` for untrusted input**:
   ```rust
   // For any untrusted input, use Rust predicates
   client.query("users")
       .where_rust(move |json| {
           // Type-safe, injection-proof
           json["status"].as_str().map(|s| s == user_status).unwrap_or(false)
       })
       .execute()
       .await?
   ```

2. **Whitelist enum values**:
   ```rust
   #[derive(Clone, Copy)]
   enum Status {
       Active,
       Inactive,
       Pending,
   }

   impl Status {
       fn to_sql(&self) -> &'static str {
           match self {
               Status::Active => "active",
               Status::Inactive => "inactive",
               Status::Pending => "pending",
           }
       }
   }

   client.query("users")
       .where_sql(&format!("data->>'status' = '{}'", status.to_sql()))
       .execute()
       .await?
   ```

3. **Escape JSON strings for WHERE clauses**:
   ```rust
   fn escape_json_string(s: &str) -> String {
       // Escape single quotes for SQL
       s.replace("'", "''")
   }

   let name = escape_json_string(&user_input_name);
   client.query("users")
       .where_sql(&format!("data->>'name' = '{}'", name))
       .execute()
       .await?
   ```

---

## Credential Management

### Passwords

#### Do NOT

- ❌ Embed passwords in source code
- ❌ Commit passwords to version control
- ❌ Include passwords in logs or error messages
- ❌ Transmit passwords over unencrypted TCP (unless using VPN/tunnel)

#### Do

- ✅ Load from environment variables
- ✅ Load from secure credential store (AWS Secrets Manager, HashiCorp Vault, etc.)
- ✅ Use Unix sockets for local connections
- ✅ Use VPN/SSH tunnel for remote TCP connections
- ✅ Rotate passwords regularly

#### Example: Environment Variables

```rust
use std::env;

fn main() -> Result<()> {
    // Load password from environment
    let password = env::var("DATABASE_PASSWORD")
        .expect("DATABASE_PASSWORD environment variable not set");

    let connection_str = format!(
        "postgres://{}@localhost/mydb",
        env::var("DATABASE_USER").unwrap_or_else(|_| "postgres".into())
    );

    // Password never appears in source code or logs
    let mut config = ConnectionConfig::from_connection_string(&connection_str)?;
    config = config.password(&password);

    Ok(())
}
```

#### Example: Secure Credential Store

```rust
use std::process::Command;

fn main() -> Result<()> {
    // Load from AWS Secrets Manager (or similar)
    let output = Command::new("aws")
        .args(&["secretsmanager", "get-secret-value", "--secret-id", "fraiseql/db"])
        .output()?;

    let secret = serde_json::from_slice::<Secret>(&output.stdout)?;
    let config = ConnectionConfig::new(&secret.db, &secret.user)
        .password(&secret.password);

    Ok(())
}
```

### Connection Strings

Connection strings may contain passwords. Be careful where you store them:

```rust
// ❌ BAD: Hardcoded in source
let client = FraiseClient::connect("postgres://user:password@host/db").await?;

// ❌ BAD: In config files checked into git
// (even if .gitignored, still risky)

// ✅ GOOD: Environment variable
let conn_str = std::env::var("FRAISEQL_URL")?;
let client = FraiseClient::connect(&conn_str).await?;

// ✅ GOOD: Construct without embedding password
let user = std::env::var("DB_USER")?;
let host = "localhost";  // Or from env
let db = std::env::var("DB_NAME")?;
let conn_str = format!("postgres://{}@{}/{}", user, host, db);
let mut config = ConnectionConfig::from_connection_string(&conn_str)?;
config = config.password(&std::env::var("DB_PASSWORD")?);
```

---

## Known Limitations

### Authentication

- **Cleartext passwords only** (v0.1.0)
  - Passwords transmitted in plain text over TCP
  - ⚠️ Only safe for localhost or VPN-protected connections
  - TLS support coming in Phase 8

- **No MD5 authentication**
  - Intentionally unsupported (MD5 is cryptographically broken)
  - Use Postgres with MD5 disabled (`password_encryption = scram-sha-256`)

- **No SCRAM authentication** (v0.1.0)
  - Coming in Phase 8
  - Will be more secure than cleartext

### Query Capabilities

- **Simple Query protocol only**
  - No parameterized queries (Extended Query protocol)
  - No prepared statements
  - No transactions

- **Single column results**
  - Must return a single `data` column (json or jsonb)
  - Read-only (SELECT only, no INSERT/UPDATE/DELETE)

### Timeouts

- **No query timeout** (v0.1.0)
  - Postgres enforces statement_timeout
  - Configure via connection params or server settings
  - Client-side timeout coming in Phase 8

---

## Security Checklist for Production

Before deploying fraiseql-wire to production, ensure:

- [ ] **Transport**: Using Unix sockets (local) OR VPN/SSH tunnel (remote)
- [ ] **Passwords**: Loaded from environment variables or credential store
- [ ] **Connection strings**: Never hardcoded or checked into git
- [ ] **Queries**: Validated for SQL injection (whitelist/Rust predicates)
- [ ] **Logging**: No passwords in logs or error messages
- [ ] **TLS plan**: Plan for TLS implementation when available
- [ ] **Dependencies**: Run `cargo audit` regularly
- [ ] **Access control**: Postgres permissions restrict user privileges
- [ ] **Monitoring**: Log and alert on authentication failures
- [ ] **Backups**: Postgres backups secured and tested

---

## Reporting Security Issues

If you discover a security vulnerability in fraiseql-wire:

1. **Do NOT** create a public GitHub issue
2. **Email** the maintainers with:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

3. **Allow time** for the team to prepare a patch before disclosure
4. **Coordinated disclosure**: Vulnerability details kept confidential until patch released

Email: [maintainers@fraiseql.dev] (placeholder - update with real address)

---

## Further Reading

- **SECURITY_AUDIT.md** - Detailed security audit findings
- **ROADMAP.md** - Future security features (TLS, SCRAM, query timeouts)
- **Postgres Security** - https://www.postgresql.org/docs/current/sql-syntax.html

---

## Questions?

If you have security questions:
1. Check this document first
2. Review SECURITY_AUDIT.md for detailed findings
3. Check GitHub Discussions
4. Email maintainers for sensitive questions
