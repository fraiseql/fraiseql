<!-- Skip to main content -->
---
title: FraiseQL Authentication Troubleshooting Guide
description: Common issues and solutions for FraiseQL authentication.
keywords: ["framework", "sdk", "monitoring", "database", "authentication"]
tags: ["documentation", "reference"]
---

# FraiseQL Authentication Troubleshooting Guide

Common issues and solutions for FraiseQL authentication.

## Login Issues

### "Invalid Redirect URI" Error

**Symptoms**: OAuth provider returns "Invalid Redirect URI"

**Causes**:

- Redirect URI not registered with provider
- Protocol mismatch (http vs https)
- Port number incorrect
- Trailing slash mismatch

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check configured redirect URI
echo $OAUTH_REDIRECT_URI

# Compare with provider settings
# Google Cloud Console → OAuth Credentials
# Keycloak → Client Settings → Valid Redirect URIs
# Auth0 → Application Settings → Allowed Callback URLs

# Exact match required:
# ❌ http://localhost:8000/auth/callback  (different port)
# ✅ http://localhost:8000/auth/callback

# ❌ http://example.com/auth/callback     (no https)
# ✅ https://example.com/auth/callback
```text
<!-- Code example in TEXT -->

### "Invalid State" Error

**Symptoms**: After OAuth provider redirects, getting "Invalid State" error

**Causes**:

- State parameter expired (>10 minutes)
- User took too long to authenticate
- State cache cleared
- Multiple browsers/tabs

**Solutions**:

```bash
<!-- Code example in BASH -->
# Increase state expiry if needed (edit auth/handlers.rs)
// 10 minutes * 60 = 600 seconds
state_expiry = now + 600;

# If state keeps expiring, check for:
# - Server clock skew
# - Network delays
# - Browser/user delay

# Test with shorter timeout:
# 1. Start auth flow
# 2. Authenticate immediately
# 3. Should work

# If works, increase user time allowance
```text
<!-- Code example in TEXT -->

### "Invalid Code" or "Code Expired"

**Symptoms**: Authorization code rejected by OAuth provider

**Causes**:

- Code already used
- Code expired (>10 minutes)
- Wrong client credentials
- Network issues during exchange

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check client credentials
echo "Client ID: $GOOGLE_CLIENT_ID"
echo "Client Secret length: ${#GOOGLE_CLIENT_SECRET}"

# Verify they match provider exactly
# Don't copy-paste manually - download from provider

# Check logs for errors
docker logs FraiseQL | grep -i "exchange"
RUST_LOG=debug cargo run

# If network issue, check:
# - DNS resolution to provider
# - TLS certificate validity
# - Network connectivity

curl -v https://oauth2.googleapis.com/token
```text
<!-- Code example in TEXT -->

### "User Not Found" or "Invalid Credentials"

**Symptoms**: User can see OAuth provider login, but fails there

**Causes**:

- User account doesn't exist
- Wrong username/password
- Account locked/disabled
- Provider not recognizing user

**Solutions**:

```bash
<!-- Code example in BASH -->
# For Google:
# - Verify Google account exists
# - Check if 2FA enabled (may need app password)
# - Try incognito mode

# For Keycloak:
# - Verify user created in realm
# - Check user enabled
# - Verify password correct
# - Check user federation if using LDAP

# For Auth0:
# - Verify user exists in Auth0 dashboard
# - Check user is not blocked
# - If using DB connection, check it's enabled
# - If using social login, check it's enabled
```text
<!-- Code example in TEXT -->

## Token Issues

### "Token Expired" on Valid Token

**Symptoms**: Token was just issued but getting "Token Expired" error

**Causes**:

- Server clock skew
- Token actually expired
- Wrong JWT issuer
- Validation config mismatch

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check server clock
date -u
# Should be within ±30 seconds of NTP server

# Fix if needed
sudo ntpdate -s time.nist.gov
sudo systemctl restart chrony

# Check JWT issuer matches provider
echo "Configured: $JWT_ISSUER"
echo "Provider issuer: https://accounts.google.com"  # example

# Decode token and check exp claim
# Use jwt.io or:
python3 -c "
import json, base64
token = 'your_token_here'
parts = token.split('.')
payload = json.loads(base64.urlsafe_b64decode(parts[1] + '=='))
import time
print(f'Expires in: {payload[\"exp\"] - int(time.time())} seconds')
"
```text
<!-- Code example in TEXT -->

### "Invalid Signature" on Token

**Symptoms**: Token rejected with "Invalid Signature"

**Causes**:

- Public key mismatch
- Token modified
- Wrong algorithm
- Key rotation issue

**Solutions**:

```bash
<!-- Code example in BASH -->
# Verify public key endpoint
curl https://accounts.google.com/oauth2/v1/certs | jq .

# Check algorithm configured
echo "JWT_ALGORITHM: $JWT_ALGORITHM"
# Should be RS256 for OAuth providers

# Verify issuer matches
echo "JWT_ISSUER: $JWT_ISSUER"

# If key rotation happened:
# - Clear any cached keys
# - Restart server
# - Fetch new keys from provider

# Restart to clear caches:
docker restart FraiseQL
```text
<!-- Code example in TEXT -->

### Can't Refresh Token

**Symptoms**: Refresh endpoint returns "Token Not Found"

**Causes**:

- Refresh token revoked
- Session expired
- Database connection issue
- Wrong token format

**Solutions**:

```bash
<!-- Code example in BASH -->
# Verify refresh token format
# Should start with base64 characters

# Check session exists in database
docker exec FraiseQL-db psql -U fraiseql_app -d FraiseQL -c \
  "SELECT COUNT(*) FROM _system.sessions;"

# Verify database connection
echo $DATABASE_URL

# Check if token was revoked
docker exec FraiseQL-db psql -U fraiseql_app -d FraiseQL -c \
  "SELECT revoked_at FROM _system.sessions LIMIT 1;"

# If revoked, need to log in again
```text
<!-- Code example in TEXT -->

## Database Issues

### "Connection Refused"

**Symptoms**: Server fails to start, "Connection refused" to database

**Causes**:

- Database not running
- Wrong host/port
- Firewall blocking
- Wrong credentials

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check database is running
docker-compose ps postgres

# Check connection string
echo $DATABASE_URL
# Should be: postgres://user:pass@host:5432/dbname

# Test connection directly
psql $DATABASE_URL -c "SELECT 1;"

# If still fails:
# Check firewall
telnet prod-db.internal 5432

# Check credentials
echo "User: fraiseql_app"
echo "Password length: ${#DATABASE_PASSWORD}"

# Try with more verbose output
RUST_LOG=debug cargo run
```text
<!-- Code example in TEXT -->

### "FATAL: database does not exist"

**Symptoms**: Server says database not found

**Solutions**:

```bash
<!-- Code example in BASH -->
# Create database if missing
docker exec FraiseQL-db psql -U postgres -c \
  "CREATE DATABASE FraiseQL;"

# Verify table exists
psql $DATABASE_URL -c "\dt _system.sessions;"

# If not, create it
psql $DATABASE_URL < /path/to/init.sql
```text
<!-- Code example in TEXT -->

### "Table does not exist"

**Symptoms**: Errors about missing `_system.sessions` table

**Solutions**:

```bash
<!-- Code example in BASH -->
# Create sessions table
psql $DATABASE_URL << EOF
CREATE TABLE IF NOT EXISTS _system.sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    refresh_token_hash TEXT NOT NULL UNIQUE,
    issued_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_sessions_user_id ON _system.sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON _system.sessions(expires_at);
CREATE INDEX idx_sessions_revoked_at ON _system.sessions(revoked_at);
EOF

# Verify
psql $DATABASE_URL -c "\d _system.sessions;"
```text
<!-- Code example in TEXT -->

## Performance Issues

### Login Is Slow

**Symptoms**: OAuth flow takes >2 seconds

**Causes**:

- OAuth provider slow
- Network latency
- Database slow
- Server overloaded

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check provider latency
time curl -I https://accounts.google.com/

# Check database query time
docker exec FraiseQL-db psql -U fraiseql_app -d FraiseQL -c \
  "SELECT query, calls, total_time/calls as avg_time \
   FROM pg_stat_statements \
   ORDER BY avg_time DESC LIMIT 10;"

# Check server metrics
curl http://localhost:8000/metrics/auth

# Look for high session lookup times
# If >50ms, increase database pool:
DATABASE_POOL_SIZE=50

# If still slow, enable query logging:
RUST_LOG=debug
```text
<!-- Code example in TEXT -->

### High CPU Usage

**Symptoms**: Server using >80% CPU

**Causes**:

- Many simultaneous logins
- Infinite loop in validation
- Key rotation loop
- Brute force attack

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check active connections
docker exec FraiseQL-db psql -U fraiseql_app -d FraiseQL -c \
  "SELECT count(*) FROM pg_stat_activity;"

# Check for brute force attempts
docker logs FraiseQL | grep "failed\|error" | tail -20

# Enable rate limiting to prevent abuse:
# Nginx rate limit:
limit_req_zone $binary_remote_addr zone=auth:10m rate=1r/s;

# Check for validation loop
# (shouldn't happen with current implementation)

# If legitimate traffic, scale horizontally:
# Add more server instances
```text
<!-- Code example in TEXT -->

### High Memory Usage

**Symptoms**: Memory grows over time or reaches limit

**Causes**:

- Sessions not expiring
- Memory leak in dependencies
- Cache growing unbounded

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check session count
docker exec FraiseQL-db psql -U fraiseql_app -d FraiseQL -c \
  "SELECT COUNT(*) FROM _system.sessions \
   WHERE revoked_at IS NULL;"

# Clean old sessions
docker exec FraiseQL-db psql -U fraiseql_app -d FraiseQL -c \
  "DELETE FROM _system.sessions \
   WHERE expires_at < $(date +%s) - 604800;"

# Restart to clear any temporary caches
docker restart FraiseQL

# Set memory limits
docker update --memory 512m FraiseQL
```text
<!-- Code example in TEXT -->

## OAuth Provider Issues

### "OAuth Provider Unreachable"

**Symptoms**: Can't connect to OAuth provider

**Causes**:

- Provider down
- Network connectivity
- Firewall/proxy blocking
- DNS resolution failure

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check provider status
curl https://accounts.google.com/o/oauth2/v2/auth?client_id=test

# Check DNS resolution
nslookup accounts.google.com

# Check network connectivity
ping accounts.google.com

# Check firewall rules
sudo ufw status

# If behind proxy:
export https_proxy=http://proxy.internal:3128
```text
<!-- Code example in TEXT -->

### "Cannot Get Public Keys"

**Symptoms**: JWT validation fails, can't fetch public keys

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check OIDC metadata endpoint
curl https://accounts.google.com/.well-known/openid-configuration

# Check JWKS endpoint
curl https://www.googleapis.com/oauth2/v1/certs

# If both respond, clear local cache and restart:
docker restart FraiseQL

# Check for certificate issues
curl -v https://accounts.google.com/ 2>&1 | grep "certificate"
```text
<!-- Code example in TEXT -->

## Debugging

### Enable Debug Logging

```bash
<!-- Code example in BASH -->
# In .env or command line
RUST_LOG=debug,fraiseql_server::auth=trace

# Or more selective
RUST_LOG=fraiseql_server::auth::handlers=debug
```text
<!-- Code example in TEXT -->

### Check Detailed Logs

```bash
<!-- Code example in BASH -->
# View real-time logs
docker logs -f FraiseQL

# Save logs to file
docker logs FraiseQL > logs.txt 2>&1

# Search logs
docker logs FraiseQL | grep "error\|warn"

# Get metrics
curl http://localhost:8000/metrics/auth | json_pp
```text
<!-- Code example in TEXT -->

### Test Endpoints Manually

```bash
<!-- Code example in BASH -->
# Start login
curl -X POST http://localhost:8000/auth/start \
  -H "Content-Type: application/json" \
  -d '{"provider":"google"}' | jq .

# List sessions
psql $DATABASE_URL -c \
  "SELECT user_id, created_at, expires_at FROM _system.sessions LIMIT 5;"

# Check health
curl http://localhost:8000/health/auth | jq .
```text
<!-- Code example in TEXT -->

## Getting Help

1. **Check logs first**: `docker logs FraiseQL`
2. **Enable debug logging**: `RUST_LOG=debug`
3. **Check GitHub issues**: <https://github.com/FraiseQL/FraiseQL/issues>
4. **Create issue with**:
   - Error message (no secrets!)
   - Steps to reproduce
   - Environment (OS, Rust version, etc.)
   - Logs with debug enabled

---

See Also:

- [Deployment Guide](./deployment.md)
- [Monitoring Guide](./monitoring.md)
- [Security Checklist](./security-checklist.md)
- [API Reference](./api-reference.md)
