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
psql --version
# or from within psql:
SELECT version();
```

### User Password Configuration

Ensure PostgreSQL is configured to use SCRAM for password authentication:

```sql
-- Check current password_encryption setting
SHOW password_encryption;
-- Output: scram-sha-256 (or md5, which is deprecated)

-- Set to SCRAM-SHA-256
ALTER SYSTEM SET password_encryption = 'scram-sha-256';
SELECT pg_reload_conf();

-- Reset user password to apply new authentication method
ALTER USER fraiseql_user PASSWORD 'new_secure_password';
```

## Configuration

### Basic SCRAM Authentication

```toml
[database]
# PostgreSQL connection string with SCRAM authentication
url = "postgresql://fraiseql_user:password@localhost:5432/fraiseql_db"
```

FraiseQL automatically:

1. Extracts the username and password from the connection string
2. Negotiates SCRAM-SHA-256 with the PostgreSQL server
3. Performs secure challenge-response authentication
4. Encrypts the password during transmission (never sent in plain text)

### With TLS/SSL (SCRAM-SHA-256-PLUS)

For the most secure configuration with channel binding:

```toml
[database]
# PostgreSQL connection string with SCRAM-SHA-256-PLUS
url = "postgresql://fraiseql_user:password@localhost:5432/fraiseql_db"

[tls]
enabled = true
ca_cert = "/path/to/ca.crt"
client_cert = "/path/to/client.crt"
client_key = "/path/to/client.key"
```

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
# On PostgreSQL server
sudo systemctl stop postgresql
sudo apt-get install postgresql-14  # or later version
sudo systemctl start postgresql
```

### Step 2: Configure SCRAM

```sql
-- Connect as superuser
sudo -u postgres psql

-- Set SCRAM as default
ALTER SYSTEM SET password_encryption = 'scram-sha-256';
SELECT pg_reload_conf();
```

### Step 3: Reset User Passwords

```sql
-- For each user, reset password to apply SCRAM
ALTER USER fraiseql_user PASSWORD 'secure_password';
ALTER USER other_user PASSWORD 'their_secure_password';
```

### Step 4: Verify Migration

```sql
-- Check that users are now using SCRAM
SELECT usename, usepassword FROM pg_user WHERE usename = 'fraiseql_user';
-- Output should show: $SCRAM-SHA-256$... (not md5...)
```

### Step 5: Update FraiseQL Configuration

```toml
# Update connection string if credentials changed
[database]
url = "postgresql://fraiseql_user:new_password@localhost:5432/fraiseql_db"
```

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
# Test with psql first
psql postgresql://fraiseql_user:password@localhost:5432/fraiseql_db

# If successful, FraiseQL should also connect
FraiseQL-server start
```

### Test with FraiseQL

```rust
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
```

### Verify Authentication Method

```bash
# View the authentication message from server
RUST_LOG=fraiseql_wire::auth=debug FraiseQL-server start

# Should show:
# [DEBUG] AuthenticationSASL with mechanisms: ["SCRAM-SHA-256"]
# [DEBUG] SCRAM-SHA-256 authentication successful
```

## Troubleshooting

### "SCRAM authentication failed"

**Possible causes:**

1. Wrong password
2. User doesn't exist in PostgreSQL
3. PostgreSQL not configured for SCRAM
4. Network connectivity issues

**Solution:**

```bash
# Test with psql first
psql postgresql://fraiseql_user:password@localhost:5432/fraiseql_db

# Check PostgreSQL logs
tail -f /var/log/postgresql/postgresql.log
```

### "Password authentication failed"

**Possible cause:** PostgreSQL still using MD5

**Solution:**

```sql
-- Check authentication method
SHOW password_encryption;

-- If output is 'md5', migrate to SCRAM:
ALTER SYSTEM SET password_encryption = 'scram-sha-256';
SELECT pg_reload_conf();
ALTER USER fraiseql_user PASSWORD 'new_password';
```

### "Connection refused"

**Possible causes:**

1. PostgreSQL not running
2. Firewall blocking port 5432
3. PostgreSQL listening on different interface

**Solution:**

```bash
# Check if PostgreSQL is running
systemctl status postgresql

# Check what PostgreSQL is listening on
ss -tlnp | grep postgres
# or
netstat -tlnp | grep postgres
```

## Performance Considerations

SCRAM authentication adds minimal overhead:

- **Handshake time**: ~10-50ms depending on network
- **PBKDF2 iterations**: Default 4096 (can be tuned)
- **Memory**: ~1KB per authentication session
- **CPU**: Negligible (modern CPUs handle SHA-256 efficiently)

For applications with thousands of connections, use connection pooling:

```toml
[database]
url = "postgresql://..."

[pool]
max_connections = 100
min_idle = 10
```

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
# Don't put passwords in code
export DATABASE_URL="postgresql://fraiseql_user:password@localhost:5432/fraiseql_db"

# Or use a secrets manager
export DATABASE_PASSWORD=$(vault kv get -field=password secret/FraiseQL/db)
```

## References

- [PostgreSQL Documentation: SCRAM-SHA-256 Authentication](https://www.postgresql.org/docs/current/sql-syntax-lexical.html)
- [RFC 5802: SCRAM (Salted Challenge Response Authentication Mechanism)](https://tools.ietf.org/html/rfc5802)
- [OWASP: Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
- [PostgreSQL Security Documentation](https://www.postgresql.org/docs/current/sql-createrole.html)
