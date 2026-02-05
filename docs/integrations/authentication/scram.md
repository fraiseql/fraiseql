<!-- Skip to main content -->
---
title: PostgreSQL SCRAM Authentication
description: FraiseQL supports SCRAM (Salted Challenge Response Authentication Mechanism) authentication for secure connections to PostgreSQL databases.
keywords: ["framework", "sdk", "monitoring", "database", "authentication"]
tags: ["documentation", "reference"]
---

# PostgreSQL SCRAM Authentication

FraiseQL supports SCRAM (Salted Challenge Response Authentication Mechanism) authentication for secure connections to PostgreSQL databases.

## Overview

SCRAM is the modern, secure authentication method for PostgreSQL. It replaces the older, vulnerable MD5 authentication scheme.

### SCRAM Variants

| Method | RFC | PostgreSQL | Channel Binding | Security |
|--------|-----|-----------|-----------------|----------|
| **SCRAM-SHA-256** | [RFC 5802](https://tools.ietf.org/html/rfc5802) | 10+ | ❌ No | ✅ **Recommended** |
| **SCRAM-SHA-256-PLUS** | [RFC 5802](https://tools.ietf.org/html/rfc5802) | 11+ | ✅ Yes | ✅ **Best** |

## Prerequisites

### PostgreSQL Version Requirements

- **SCRAM-SHA-256**: PostgreSQL 10 or later
- **SCRAM-SHA-256-PLUS**: PostgreSQL 11 or later (with TLS channel binding)

Check your PostgreSQL version:

```bash
<!-- Code example in BASH -->
psql --version
# or from within psql:
SELECT version();
```text
<!-- Code example in TEXT -->

### User Password Configuration

Ensure PostgreSQL is configured to use SCRAM for password authentication:

```sql
<!-- Code example in SQL -->
-- Check current password_encryption setting
SHOW password_encryption;
-- Output: scram-sha-256 (or md5, which is deprecated)

-- Set to SCRAM-SHA-256
ALTER SYSTEM SET password_encryption = 'scram-sha-256';
SELECT pg_reload_conf();

-- Reset user password to apply new authentication method
ALTER USER fraiseql_user PASSWORD 'new_secure_password';
```text
<!-- Code example in TEXT -->

## Configuration

### Basic SCRAM Authentication

```toml
<!-- Code example in TOML -->
[database]
# PostgreSQL connection string with SCRAM authentication
url = "postgresql://fraiseql_user:password@localhost:5432/fraiseql_db"
```text
<!-- Code example in TEXT -->

FraiseQL automatically:

1. Extracts the username and password from the connection string
2. Negotiates SCRAM-SHA-256 with the PostgreSQL server
3. Performs secure challenge-response authentication
4. Encrypts the password during transmission (never sent in plain text)

### With TLS/SSL (SCRAM-SHA-256-PLUS)

For the most secure configuration with channel binding:

```toml
<!-- Code example in TOML -->
[database]
# PostgreSQL connection string with SCRAM-SHA-256-PLUS
url = "postgresql://fraiseql_user:password@localhost:5432/fraiseql_db"

[tls]
enabled = true
ca_cert = "/path/to/ca.crt"
client_cert = "/path/to/client.crt"
client_key = "/path/to/client.key"
```text
<!-- Code example in TEXT -->

When both TLS and SCRAM are enabled:

1. TLS connection is established first
2. SCRAM-SHA-256-PLUS negotiates with TLS channel binding
3. Authentication is tied to the TLS session (prevents replay attacks)

## Security Features

### SCRAM-SHA-256

✅ **Advantages:**

- Password is never sent in plaintext
- Uses SHA-256 hashing with salt
- Each password is hashed with a unique salt
- Multiple iterations prevent dictionary attacks (default: 4096 PBKDF2 iterations)
- Resistant to brute-force attacks
- Resistant to timing attacks

✅ **Protection Against:**

- Plaintext password exposure
- Rainbow tables
- Dictionary attacks
- Brute-force attacks
- Timing-based password guessing

### SCRAM-SHA-256-PLUS

Adds **channel binding** for additional security:

✅ **Additional Protection:**

- Binds authentication to TLS certificate
- Prevents MITM attacks on the authentication layer
- Prevents key compromise attacks
- Authenticates the server through the TLS channel

## Migration from MD5

If your PostgreSQL instance uses MD5 authentication:

### Step 1: Update PostgreSQL

```bash
<!-- Code example in BASH -->
# On PostgreSQL server
sudo systemctl stop postgresql
sudo apt-get install postgresql-14  # or later version
sudo systemctl start postgresql
```text
<!-- Code example in TEXT -->

### Step 2: Configure SCRAM

```sql
<!-- Code example in SQL -->
-- Connect as superuser
sudo -u postgres psql

-- Set SCRAM as default
ALTER SYSTEM SET password_encryption = 'scram-sha-256';
SELECT pg_reload_conf();
```text
<!-- Code example in TEXT -->

### Step 3: Reset User Passwords

```sql
<!-- Code example in SQL -->
-- For each user, reset password to apply SCRAM
ALTER USER fraiseql_user PASSWORD 'secure_password';
ALTER USER other_user PASSWORD 'their_secure_password';
```text
<!-- Code example in TEXT -->

### Step 4: Verify Migration

```sql
<!-- Code example in SQL -->
-- Check that users are now using SCRAM
SELECT usename, usepassword FROM pg_user WHERE usename = 'fraiseql_user';
-- Output should show: $SCRAM-SHA-256$... (not md5...)
```text
<!-- Code example in TEXT -->

### Step 5: Update FraiseQL Configuration

```toml
<!-- Code example in TOML -->
# Update connection string if credentials changed
[database]
url = "postgresql://fraiseql_user:new_password@localhost:5432/fraiseql_db"
```text
<!-- Code example in TEXT -->

## Password Requirements

FraiseQL securely handles passwords:

### Recommendations

1. **Strong Passwords**: Use 16+ character passwords with mixed case, numbers, and symbols
2. **Unique Passwords**: Don't reuse passwords across systems
3. **Regular Rotation**: Change database passwords every 90 days
4. **No Logging**: Passwords are never logged or printed to console

### Memory Security

FraiseQL uses the `zeroize` crate to:

- Immediately zero password memory after use
- Prevent password fragments in memory dumps
- Automatic on drop (no manual cleanup needed)

## Testing SCRAM Configuration

### Test Connection

```bash
<!-- Code example in BASH -->
# Test with psql first
psql postgresql://fraiseql_user:password@localhost:5432/fraiseql_db

# If successful, FraiseQL should also connect
FraiseQL-server start
```text
<!-- Code example in TEXT -->

### Test with FraiseQL

```rust
<!-- Code example in RUST -->
use fraiseql_core::database::ConnectionPool;

#[tokio::main]
async fn main() {
    let config = ConnectionConfig {
        database_url: "postgresql://fraiseql_user:password@localhost:5432/fraiseql_db".to_string(),
        ..Default::default()
    };

    let pool = ConnectionPool::new(config).await.expect("Connection failed");
    println!("✅ SCRAM authentication successful");
}
```text
<!-- Code example in TEXT -->

### Verify Authentication Method

```bash
<!-- Code example in BASH -->
# View the authentication message from server
RUST_LOG=fraiseql_wire::auth=debug FraiseQL-server start

# Should show:
# [DEBUG] AuthenticationSASL with mechanisms: ["SCRAM-SHA-256"]
# [DEBUG] SCRAM-SHA-256 authentication successful
```text
<!-- Code example in TEXT -->

## Troubleshooting

### "SCRAM authentication failed"

**Possible causes:**

1. Wrong password
2. User doesn't exist in PostgreSQL
3. PostgreSQL not configured for SCRAM
4. Network connectivity issues

**Solution:**

```bash
<!-- Code example in BASH -->
# Test with psql first
psql postgresql://fraiseql_user:password@localhost:5432/fraiseql_db

# Check PostgreSQL logs
tail -f /var/log/postgresql/postgresql.log
```text
<!-- Code example in TEXT -->

### "Password authentication failed"

**Possible cause:** PostgreSQL still using MD5

**Solution:**

```sql
<!-- Code example in SQL -->
-- Check authentication method
SHOW password_encryption;

-- If output is 'md5', migrate to SCRAM:
ALTER SYSTEM SET password_encryption = 'scram-sha-256';
SELECT pg_reload_conf();
ALTER USER fraiseql_user PASSWORD 'new_password';
```text
<!-- Code example in TEXT -->

### "Connection refused"

**Possible causes:**

1. PostgreSQL not running
2. Firewall blocking port 5432
3. PostgreSQL listening on different interface

**Solution:**

```bash
<!-- Code example in BASH -->
# Check if PostgreSQL is running
systemctl status postgresql

# Check what PostgreSQL is listening on
ss -tlnp | grep postgres
# or
netstat -tlnp | grep postgres
```text
<!-- Code example in TEXT -->

## Performance Considerations

SCRAM authentication adds minimal overhead:

- **Handshake time**: ~10-50ms depending on network
- **PBKDF2 iterations**: Default 4096 (can be tuned)
- **Memory**: ~1KB per authentication session
- **CPU**: Negligible (modern CPUs handle SHA-256 efficiently)

For applications with thousands of connections, use connection pooling:

```toml
<!-- Code example in TOML -->
[database]
url = "postgresql://..."

[pool]
max_connections = 100
min_idle = 10
```text
<!-- Code example in TEXT -->

## Security Best Practices

1. **Use SCRAM-SHA-256-PLUS** when possible (with TLS)
2. **Enable TLS** for all connections to PostgreSQL
3. **Use strong passwords** (16+ characters, mixed case)
4. **Rotate passwords regularly** (every 90 days)
5. **Monitor authentication failures** in PostgreSQL logs
6. **Separate database credentials** from application code (use environment variables)
7. **Use connection pooling** to limit authentication overhead
8. **Audit authentication events** for security monitoring

### Environment Variables

```bash
<!-- Code example in BASH -->
# Don't put passwords in code
export DATABASE_URL="postgresql://fraiseql_user:password@localhost:5432/fraiseql_db"

# Or use a secrets manager
export DATABASE_PASSWORD=$(vault kv get -field=password secret/FraiseQL/db)
```text
<!-- Code example in TEXT -->

## References

- [PostgreSQL Documentation: SCRAM-SHA-256 Authentication](https://www.postgresql.org/docs/current/sql-syntax-lexical.html)
- [RFC 5802: SCRAM (Salted Challenge Response Authentication Mechanism)](https://tools.ietf.org/html/rfc5802)
- [OWASP: Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
- [PostgreSQL Security Documentation](https://www.postgresql.org/docs/current/sql-createrole.html)
