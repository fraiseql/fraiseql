<!-- Skip to main content -->
---

title: PostgreSQL Authentication Guide
description: FraiseQL requires secure authentication with PostgreSQL using SCRAM-based mechanisms. This guide covers supported authentication methods and version requirement
keywords: []
tags: ["documentation", "reference"]
---

# PostgreSQL Authentication Guide

FraiseQL requires secure authentication with PostgreSQL using SCRAM-based mechanisms. This guide covers supported authentication methods and version requirements.

## Prerequisites

**Required Knowledge:**

- PostgreSQL user and role management
- SCRAM authentication protocol basics
- SSL/TLS certificate handling
- Connection string/URI syntax
- Database permissions and privilege models
- Linux/Unix command-line tools (psql, openssl)

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- PostgreSQL 10+ (for SCRAM-SHA-256 support)
- psql command-line client (usually included with PostgreSQL)
- OpenSSL 1.1.1+ (for certificate generation)
- A text editor for configuration files

**Required Infrastructure:**

- PostgreSQL 10 or later instance (local or remote)
- PostgreSQL superuser or admin account for user creation
- FraiseQL server instance
- Network connectivity between FraiseQL and PostgreSQL
- For TLS: PostgreSQL compiled with SSL support

**Optional but Recommended:**

- PostgreSQL HA solution (replication, failover)
- Connection pooling (pgBouncer, PgPool)
- Secrets management system (Vault, AWS Secrets Manager)
- Monitoring tools (pg_stat_statements, pg_stat_monitor)
- Audit logging for authentication events

**Time Estimate:** 20-40 minutes for basic setup, 1-2 hours for production TLS setup

## Overview

PostgreSQL authentication in FraiseQL uses the SCRAM (Salted Challenge Response Authentication Mechanism) family of authentication protocols. These are cryptographically secure alternatives to older MD5-based authentication.

## Supported Authentication Methods

### SCRAM-SHA-256 (Recommended)

**Status**: ✅ Recommended for production

SCRAM-SHA-256 is a salted challenge-response authentication mechanism defined in RFC 5802. It provides:

- Cryptographic security (SHA-256)
- Protection against rainbow table attacks (salt-based)
- No plaintext password transmission
- Defense against MitM attacks

**Requirements**:

- PostgreSQL 10 or later
- User password must be set using SCRAM-SHA-256

**Configuration**:

```toml
<!-- Code example in TOML -->
[database]
url = "postgresql://username:password@localhost:5432/FraiseQL"
```text
<!-- Code example in TEXT -->

### SCRAM-SHA-256-PLUS (Channel Binding)

**Status**: ✅ Best for highly sensitive deployments

SCRAM-SHA-256-PLUS adds channel binding to SCRAM-SHA-256, providing additional protection by binding the authentication to the TLS connection itself.

**Requirements**:

- PostgreSQL 11 or later
- TLS connection required
- Explicit channel binding support in driver

**When to use**:

- Multi-tenant deployments
- Highly sensitive data
- High-security compliance requirements (SOC2, ISO 27001)

**Configuration**:

```toml
<!-- Code example in TOML -->
[database]
url = "postgresql://username:password@localhost:5432/FraiseQL?sslmode=require"
```text
<!-- Code example in TEXT -->

## PostgreSQL Version Requirements

| Version | SCRAM-SHA-256 | SCRAM-SHA-256-PLUS | Notes |
|---------|---------------|--------------------|-------|
| < 10    | ❌ Not supported | ❌ Not supported | **Upgrade required** - MD5 only |
| 10-10.x | ✅ Supported | ❌ Not supported | Minimum version for SCRAM |
| 11+     | ✅ Supported | ✅ Supported | **Recommended** |
| 12+     | ✅ Supported | ✅ Supported | Current stable branch |
| 13+     | ✅ Supported | ✅ Supported | Current stable branch |
| 14+     | ✅ Supported | ✅ Supported | Current stable branch |
| 15+     | ✅ Supported | ✅ Supported | Current stable branch |
| 16+     | ✅ Supported | ✅ Supported | Current stable branch |
| 17+     | ✅ Supported | ✅ Supported | Current stable branch |

## Migration from MD5

If you're currently using older PostgreSQL versions with MD5 authentication, follow these migration steps:

### Step 1: Upgrade PostgreSQL

Upgrade to PostgreSQL 10 or later:

```bash
<!-- Code example in BASH -->
# Check current version
psql --version

# For Ubuntu/Debian
sudo apt-get update
sudo apt-get install postgresql-11  # or newer version

# For macOS with Homebrew
brew upgrade postgresql
```text
<!-- Code example in TEXT -->

### Step 2: Configure SCRAM Authentication

Update PostgreSQL configuration to enforce SCRAM:

**PostgreSQL Server Configuration** (`postgresql.conf`):

```ini
<!-- Code example in INI -->
# Enforce SCRAM for all new connections
password_encryption = 'scram-sha256'
```text
<!-- Code example in TEXT -->

### Step 3: Reset User Passwords

PostgreSQL stores password hashes. To update existing users to SCRAM:

```sql
<!-- Code example in SQL -->
-- Reset password for FraiseQL user (this will create SCRAM-SHA-256 hash)
ALTER USER fraiseql_user WITH PASSWORD 'new_secure_password';

-- For new users, this is automatic with password_encryption = 'scram-sha256'
CREATE USER fraiseql_user WITH PASSWORD 'secure_password';
```text
<!-- Code example in TEXT -->

### Step 4: Update Connection String

Update your FraiseQL configuration:

```toml
<!-- Code example in TOML -->
[database]
# Old (MD5 - deprecated)
# url = "postgresql://fraiseql_user:password@localhost:5432/FraiseQL"

# New (SCRAM-SHA-256)
url = "postgresql://fraiseql_user:secure_password@localhost:5432/FraiseQL"
```text
<!-- Code example in TEXT -->

## Verifying SCRAM Authentication

### Check PostgreSQL Server Configuration

```sql
<!-- Code example in SQL -->
-- Check password encryption method
SHOW password_encryption;
-- Should output: scram-sha256

-- Check authentication method in pg_hba.conf
SELECT * FROM pg_hba_file_rules WHERE auth_method LIKE 'scram%';
```text
<!-- Code example in TEXT -->

### Check User Authentication Method

```sql
<!-- Code example in SQL -->
-- Check a specific user's password hash (only visible to superusers)
SELECT usename, usesuper FROM pg_user WHERE usename = 'fraiseql_user';

-- The password is stored as a SCRAM hash, not MD5
SELECT substring(rolpassword, 1, 10) as hash_prefix
FROM pg_authid WHERE rolname = 'fraiseql_user';
-- Should start with "SCRAM-SHA-256" not "md5"
```text
<!-- Code example in TEXT -->

### Test Connection from FraiseQL

```bash
<!-- Code example in BASH -->
# Test the connection with verbose logging
RUST_LOG=debug FraiseQL-server start

# Look for successful SCRAM-SHA-256 authentication in logs
# Should see: "Successfully authenticated using SCRAM-SHA-256"
```text
<!-- Code example in TEXT -->

## Troubleshooting

### "FATAL: password authentication failed for user"

**Cause**: Password mismatch or authentication method incompatibility

**Solution**:

1. Verify the password is correct
2. Check PostgreSQL server is using SCRAM: `SHOW password_encryption;`
3. Reset the password: `ALTER USER fraiseql_user WITH PASSWORD 'password';`
4. Verify connection string format

### "SCRAM authentication required but not available"

**Cause**: PostgreSQL version < 10 or MD5-only configuration

**Solution**:

1. Upgrade PostgreSQL to 10+
2. Update `password_encryption` in `postgresql.conf`
3. Restart PostgreSQL: `sudo systemctl restart postgresql`
4. Reset passwords for all users

### "SCRAM-SHA-256-PLUS not supported"

**Cause**: PostgreSQL version < 11 or TLS not configured

**Solution**:

1. For SCRAM-SHA-256-PLUS, upgrade to PostgreSQL 11+
2. Enable TLS: Add `sslmode=require` to connection string
3. Verify TLS certificates are valid
4. Check PostgreSQL compiled with OpenSSL support: `SELECT ssl FROM pg_config();`

## Best Practices

1. **Always use SCRAM**: Migrate away from MD5 authentication
2. **Use Strong Passwords**: Generate cryptographically random passwords (16+ characters)
3. **Enable TLS**: Always encrypt the connection wire
4. **Separate Credentials**: Use dedicated database users for each application
5. **Rotate Passwords**: Rotate database passwords regularly (quarterly or as per policy)
6. **Monitor Authentication**: Monitor failed authentication attempts in PostgreSQL logs
7. **Use Secrets Management**: Store passwords in a secrets manager, not in plaintext config

## Example: Complete Setup

```bash
<!-- Code example in BASH -->
#!/bin/bash
# Complete PostgreSQL SCRAM setup for FraiseQL

# 1. Connect as PostgreSQL superuser
sudo -u postgres psql

# In psql:

-- Enable SCRAM for future password changes
ALTER SYSTEM SET password_encryption = 'scram-sha256';

-- Create dedicated FraiseQL user
CREATE USER fraiseql_user WITH PASSWORD 'your_secure_password_here';

-- Grant necessary permissions
GRANT CONNECT ON DATABASE FraiseQL TO fraiseql_user;
GRANT USAGE ON SCHEMA public TO fraiseql_user;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO fraiseql_user;

-- Reload configuration
SELECT pg_reload_conf();

-- Exit psql
\q

# 2. Restart PostgreSQL to apply changes
sudo systemctl restart postgresql

# 3. Verify SCRAM is enabled
sudo -u postgres psql -c "SHOW password_encryption;"
# Should output: scram-sha256

# 4. Test connection from FraiseQL
psql -h localhost -U fraiseql_user -d FraiseQL
# Should successfully authenticate with SCRAM-SHA-256
```text
<!-- Code example in TEXT -->

## Security Implications

| Aspect | MD5 (Deprecated) | SCRAM-SHA-256 | SCRAM-SHA-256-PLUS |
|--------|------------------|---------------|--------------------|
| **Cryptographic Strength** | Weak (broken) | Strong | Strong |
| **Salt Protection** | None | Per-user | Per-user |
| **Rainbow Table Resistant** | No | Yes | Yes |
| **Channel Binding** | N/A | None | TLS-bound |
| **MitM Protection** | Low | Medium | High |
| **Recommended** | ❌ Never | ✅ Production | ✅✅ Sensitive |

## References

- [PostgreSQL Authentication Documentation](https://www.postgresql.org/docs/current/auth-methods.html)
- [RFC 5802 - SCRAM](https://tools.ietf.org/html/rfc5802)
- [PostgreSQL password_encryption Parameter](https://www.postgresql.org/docs/current/runtime-config-connection.html#GUC-PASSWORD-ENCRYPTION)
- [PostgreSQL Security](https://www.postgresql.org/docs/current/sql-syntax.html)

## Support Matrix

| Component | PostgreSQL 10 | PostgreSQL 11+ | Notes |
|-----------|---------------|----------------|-------|
| FraiseQL Core | ✅ Supported | ✅ Recommended | Min version for SCRAM |
| SCRAM-SHA-256 | ✅ Yes | ✅ Yes | Recommended auth |
| SCRAM-SHA-256-PLUS | ❌ No | ✅ Yes | Best security |
| Connection Pooling | ✅ Yes | ✅ Yes | Via pgBouncer |
| Replication | ✅ Yes | ✅ Yes | Streaming replication |
