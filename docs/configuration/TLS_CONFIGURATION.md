# TLS/SSL Configuration Guide

## Prerequisites

**Required Knowledge:**

- SSL/TLS fundamentals (certificates, keys, handshakes)
- X.509 certificate structure and standards
- Public Key Infrastructure (PKI) concepts
- OpenSSL command-line tools
- DNS and certificate CN/SAN matching
- Basic Linux/Unix system administration

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- OpenSSL 1.1.1+ (for certificate generation and verification)
- curl or OpenSSL CLI (for testing HTTPS endpoints)
- A code editor for configuration files
- Bash or similar shell for scripting

**Required Infrastructure:**

- FraiseQL server instance (local or deployed)
- TLS certificate and private key (self-signed or from CA)
- Ports 8443 (HTTPS) and optionally 8444 (mTLS client cert)
- Database with TLS support (PostgreSQL 10+, MySQL 5.7+, etc.)

**Optional but Recommended:**

- Certificate Authority (CA) certificate for client validation
- Let's Encrypt or other automated certificate provisioning
- HSM (Hardware Security Module) for key storage
- Certificate management tools (cert-manager for Kubernetes)
- Nginx or Envoy reverse proxy with SSL termination

**Time Estimate:** 30-60 minutes for basic setup, 2-3 hours for mTLS in production

## Overview

FraiseQL v2 Phase 10.10 implements comprehensive TLS encryption for:

1. **HTTP/gRPC Endpoints** - HTTPS and encrypted gRPC (Arrow Flight)
2. **Mutual TLS (mTLS)** - Optional client certificate requirements
3. **Database Connections** - TLS for PostgreSQL, Redis, ClickHouse, Elasticsearch
4. **At-Rest Encryption** - Configuration hooks for database-native encryption

## Quick Start: Production Setup

### 1. Generate TLS Certificate and Key

Using OpenSSL:

```bash
# Generate private key
openssl genrsa -out /etc/fraiseql/key.pem 2048

# Generate self-signed certificate (or use your CA)
openssl req -new -x509 -key /etc/fraiseql/key.pem -out /etc/fraiseql/cert.pem \
  -subj "/CN=fraiseql.example.com/O=YourOrg/C=US"

# Set proper permissions
chmod 600 /etc/fraiseql/key.pem
chmod 644 /etc/fraiseql/cert.pem
```text

### 2. Configure fraiseql.toml

```toml
[server]
bind_address = "0.0.0.0:8443"  # HTTPS port
database_url = "postgresql://user:pass@db.example.com/fraiseql"

# TLS for HTTP/gRPC endpoints
[tls]
enabled = true
cert_path = "/etc/fraiseql/cert.pem"
key_path = "/etc/fraiseql/key.pem"
require_client_cert = false           # Set to true for mTLS
min_version = "1.2"                   # "1.2" or "1.3" (recommend 1.3)

# TLS for database connections
[database_tls]
postgres_ssl_mode = "require"         # disable, allow, prefer, require, verify-ca, verify-full
redis_ssl = true                      # Use rediss:// protocol
clickhouse_https = true               # Use HTTPS
elasticsearch_https = true            # Use HTTPS
verify_certificates = true            # Verify server certificates
ca_bundle_path = "/etc/ssl/certs/ca-bundle.crt"  # Optional: CA bundle for verification
```text

### 3. Start Server with TLS

```bash
FRAISEQL_TLS_ENABLED=true \
  FRAISEQL_TLS_CERT_PATH=/etc/fraiseql/cert.pem \
  FRAISEQL_TLS_KEY_PATH=/etc/fraiseql/key.pem \
  fraiseql-server --config fraiseql.toml
```text

## Configuration Options

### Server TLS Configuration (`[tls]`)

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | bool | `false` | Enable TLS for HTTP/gRPC endpoints |
| `cert_path` | path | Required if enabled | Path to PEM certificate file |
| `key_path` | path | Required if enabled | Path to PEM private key file |
| `require_client_cert` | bool | `false` | Require client certificates (mTLS) |
| `client_ca_path` | path | Optional | CA certificate for validating client certs (required if `require_client_cert = true`) |
| `min_version` | string | `"1.2"` | Minimum TLS version: `"1.2"` or `"1.3"` |

### Database TLS Configuration (`[database_tls]`)

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `postgres_ssl_mode` | string | `"prefer"` | PostgreSQL SSL mode: `disable`, `allow`, `prefer`, `require`, `verify-ca`, `verify-full` |
| `redis_ssl` | bool | `false` | Enable TLS for Redis (uses `rediss://` protocol) |
| `clickhouse_https` | bool | `false` | Enable HTTPS for ClickHouse |
| `elasticsearch_https` | bool | `false` | Enable HTTPS for Elasticsearch |
| `verify_certificates` | bool | `true` | Verify server certificates |
| `ca_bundle_path` | path | Optional | Path to CA certificate bundle for verification |

## Configuration Examples

### Example 1: Development (Permissive)

```toml
# Development: TLS optional, minimal validation
[tls]
enabled = false

[database_tls]
postgres_ssl_mode = "prefer"
redis_ssl = false
clickhouse_https = false
elasticsearch_https = false
verify_certificates = false
```text

### Example 2: Staging (Standard)

```toml
# Staging: TLS required, standard validation
[tls]
enabled = true
cert_path = "/etc/fraiseql/cert.pem"
key_path = "/etc/fraiseql/key.pem"
require_client_cert = false
min_version = "1.2"

[database_tls]
postgres_ssl_mode = "require"
redis_ssl = true
clickhouse_https = true
elasticsearch_https = true
verify_certificates = true
ca_bundle_path = "/etc/ssl/certs/ca-bundle.crt"
```text

### Example 3: Production (Strict mTLS)

```toml
# Production: TLS required with mTLS, strict validation
[tls]
enabled = true
cert_path = "/etc/fraiseql/cert.pem"
key_path = "/etc/fraiseql/key.pem"
require_client_cert = true
client_ca_path = "/etc/fraiseql/client-ca.pem"
min_version = "1.3"  # TLS 1.3 only

[database_tls]
postgres_ssl_mode = "verify-full"
redis_ssl = true
clickhouse_https = true
elasticsearch_https = true
verify_certificates = true
ca_bundle_path = "/etc/ssl/certs/ca-bundle.crt"
```text

## TLS Enforcement Levels

FraiseQL uses three TLS enforcement profiles:

### 1. Permissive (Development)

```rust
TlsEnforcer::permissive()
// - TLS optional for HTTP connections
// - Client certificates optional
// - TLS 1.2 minimum (if used)
```text

**Usage**: Local development, testing

### 2. Standard (Production)

```rust
TlsEnforcer::standard()
// - TLS required (HTTPS only)
// - Client certificates optional
// - TLS 1.2 minimum
```text

**Usage**: Default production setup

### 3. Strict (Regulated Environments)

```rust
TlsEnforcer::strict()
// - TLS required (HTTPS only)
// - Client certificates required (mTLS)
// - TLS 1.3 minimum
```text

**Usage**: PCI-DSS, HIPAA, SOC 2 compliance

## PostgreSQL SSL Modes

FraiseQL supports all PostgreSQL SSL modes:

| Mode | Security | Behavior |
|------|----------|----------|
| `disable` | ❌ Unsafe | No SSL, unencrypted |
| `allow` | ⚠️ Moderate | Upgrade to SSL if available |
| `prefer` | ⚠️ Moderate | Try SSL first, fall back to unencrypted |
| `require` | ✅ Good | SSL required, no fallback |
| `verify-ca` | ✅ Better | SSL + verify CA certificate |
| `verify-full` | ✅ Best | SSL + verify CA and hostname |

**Recommendation for production**: Use `require` or `verify-full`

## Database URLs with TLS

### PostgreSQL

```text
# Without TLS
postgresql://user:pass@localhost:5432/fraiseql

# With TLS (require mode)
postgresql://user:pass@localhost:5432/fraiseql?sslmode=require

# With TLS (verify-full mode)
postgresql://user:pass@localhost:5432/fraiseql?sslmode=verify-full&sslrootcert=/etc/ssl/certs/ca.pem
```text

### Redis

```text
# Without TLS
redis://localhost:6379

# With TLS (automatic with rediss:// protocol)
rediss://localhost:6379
rediss://:password@localhost:6379
```text

### ClickHouse

```text
# Without TLS
http://localhost:8123

# With TLS (HTTPS)
https://localhost:8123
```text

### Elasticsearch

```text
# Without TLS
http://localhost:9200

# With TLS (HTTPS)
https://localhost:9200
```text

## At-Rest Encryption Configuration

### ClickHouse

To enable encryption at rest in ClickHouse:

```sql
CREATE TABLE fraiseql_events (
    event_id UUID,
    org_id UUID,
    timestamp DateTime,
    data String,
    ...
) ENGINE = MergeTree()
PARTITION BY org_id
ORDER BY (org_id, timestamp)
WITH SETTINGS
    storage_disk_name = 'encrypted';
```text

**Note**: Requires ClickHouse 21.10+ and disk encryption configured

### Elasticsearch

To enable encryption at rest with ILM policy:

```json
{
  "policy": "fraiseql-policy",
  "phases": {
    "hot": {
      "min_age": "0d",
      "actions": {
        "rollover": { "max_size": "50gb" },
        "set_priority": { "priority": 100 }
      }
    },
    "warm": {
      "min_age": "7d",
      "actions": {
        "set_priority": { "priority": 50 },
        "forcemerge": { "max_num_segments": 1 }
      }
    },
    "cold": {
      "min_age": "30d",
      "actions": {
        "set_priority": { "priority": 0 },
        "searchable_snapshot": { "snapshot_repository": "found-snapshots" }
      }
    }
  }
}
```text

**Note**: Requires Elasticsearch 7.9+ and subscription-level features

## Client Certificate Generation (mTLS)

If you're using mTLS (`require_client_cert = true`):

### 1. Generate CA Key and Certificate

```bash
# CA private key
openssl genrsa -out ca-key.pem 4096

# CA certificate
openssl req -new -x509 -days 3650 -key ca-key.pem -out ca-cert.pem \
  -subj "/CN=fraiseql-ca/O=YourOrg/C=US"
```text

### 2. Generate Client Certificate

```bash
# Client key
openssl genrsa -out client-key.pem 2048

# Client CSR
openssl req -new -key client-key.pem -out client.csr \
  -subj "/CN=client.example.com/O=YourOrg/C=US"

# Sign with CA
openssl x509 -req -days 365 -in client.csr \
  -CA ca-cert.pem -CAkey ca-key.pem -CAcreateserial \
  -out client-cert.pem
```text

### 3. Configure in fraiseql.toml

```toml
[tls]
enabled = true
cert_path = "/etc/fraiseql/server-cert.pem"
key_path = "/etc/fraiseql/server-key.pem"
require_client_cert = true
client_ca_path = "/etc/fraiseql/ca-cert.pem"
min_version = "1.3"
```text

## Docker Compose Example

```yaml
version: '3.8'

services:
  fraiseql:
    build: .
    ports:
      - "8443:8443"  # HTTPS
    environment:
      DATABASE_URL: postgresql://user:pass@postgres:5432/fraiseql
      FRAISEQL_TLS_ENABLED: "true"
      FRAISEQL_TLS_CERT_PATH: /etc/fraiseql/cert.pem
      FRAISEQL_TLS_KEY_PATH: /etc/fraiseql/key.pem
    volumes:
      - ./certs:/etc/fraiseql:ro
    depends_on:
      - postgres

  postgres:
    image: postgres:16
    environment:
      POSTGRES_DB: fraiseql
      POSTGRES_PASSWORD: password
    command: >
      -c ssl=on
      -c ssl_cert_file=/var/lib/postgresql/server.crt
      -c ssl_key_file=/var/lib/postgresql/server.key
    volumes:
      - ./certs/postgres-cert.pem:/var/lib/postgresql/server.crt:ro
      - ./certs/postgres-key.pem:/var/lib/postgresql/server.key:ro

  redis:
    image: redis:7
    command: redis-server --tls-port 6379 --port 0 --tls-cert-file /etc/redis/cert.pem --tls-key-file /etc/redis/key.pem
    volumes:
      - ./certs/redis-cert.pem:/etc/redis/cert.pem:ro
      - ./certs/redis-key.pem:/etc/redis/key.pem:ro
```text

## Kubernetes TLS Configuration

### Secret Setup

```bash
# Create TLS secret
kubectl create secret tls fraiseql-tls \
  --cert=./certs/cert.pem \
  --key=./certs/key.pem
```text

### Deployment Configuration

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-server
spec:
  template:
    spec:
      containers:
      - name: fraiseql
        image: fraiseql:latest
        env:
        - name: FRAISEQL_TLS_ENABLED
          value: "true"
        - name: FRAISEQL_TLS_CERT_PATH
          value: /etc/fraiseql/tls/cert.pem
        - name: FRAISEQL_TLS_KEY_PATH
          value: /etc/fraiseql/tls/key.pem
        volumeMounts:
        - name: tls-certs
          mountPath: /etc/fraiseql/tls
          readOnly: true
      volumes:
      - name: tls-certs
        secret:
          secretName: fraiseql-tls
```text

## Verification and Testing

### 1. Test HTTPS Connection

```bash
# With curl (ignore self-signed cert warnings)
curl -k https://localhost:8443/health

# With proper CA cert
curl --cacert /etc/ssl/certs/ca-cert.pem https://localhost:8443/health

# With client certificate (mTLS)
curl \
  --cacert /etc/ssl/certs/ca-cert.pem \
  --cert /etc/fraiseql/client-cert.pem \
  --key /etc/fraiseql/client-key.pem \
  https://localhost:8443/health
```text

### 2. Test TLS Version

```bash
# Check TLS version
openssl s_client -connect localhost:8443 -tls1_2 < /dev/null

# Verify minimum version enforcement
openssl s_client -connect localhost:8443 -tls1_1 < /dev/null  # Should fail
```text

### 3. Test Database Connections

```bash
# PostgreSQL
psql "postgresql://user@localhost/fraiseql?sslmode=require"

# Redis
redis-cli --tls --cacert /etc/ssl/certs/ca-cert.pem ping

# ClickHouse
curl --cacert /etc/ssl/certs/ca-cert.pem https://localhost:8123/ping

# Elasticsearch
curl --cacert /etc/ssl/certs/ca-cert.pem https://localhost:9200/_cluster/health
```text

## Security Best Practices

1. **Use TLS 1.3** when possible (`min_version = "1.3"`)
2. **Verify certificates** for all database connections
3. **Rotate certificates** before expiration (set calendar reminders)
4. **Use strong private keys** (2048-bit RSA minimum, 4096-bit preferred)
5. **Protect certificate files** with proper permissions (`chmod 600 key.pem`)
6. **Use certificate management tools**:
   - Let's Encrypt with Certbot (free automated renewal)
   - HashiCorp Consul for certificate management
   - Kubernetes cert-manager (if using K8s)
7. **Monitor certificate expiration**:

   ```bash
   openssl x509 -enddate -noout -in /etc/fraiseql/cert.pem
   ```text

8. **Use different keys per environment** (dev, staging, production)
9. **Store private keys in secrets management** (HashiCorp Vault, AWS Secrets Manager)
10. **Enable certificate pinning** for critical database connections

## Troubleshooting

### TLS Certificate Not Found

```text
error: TLS enabled but certificate file not found: /etc/fraiseql/cert.pem
```text

**Solution**: Verify certificate path exists and is readable by the FraiseQL process

### TLS Version Too Old

```text
error: Connection TLS version (1.2) is less than minimum required (1.3)
```text

**Solution**: Update client TLS version or lower `min_version` in config

### Client Certificate Required

```text
error: Client certificate required, but none provided
```text

**Solution**: Provide client certificate if using mTLS, or disable with `require_client_cert = false`

### Database Connection SSL Error

```text
error: FATAL: no pg_hba.conf entry for replication connection
```text

**Solution**: Ensure database SSL is properly configured and using correct `sslmode`

## References

- [TLS 1.3 RFC 8446](https://tools.ietf.org/html/rfc8446)
- [OWASP Transport Layer Protection](https://cheatsheetseries.owasp.org/cheatsheets/Transport_Layer_Protection_Cheat_Sheet.html)
- [PostgreSQL SSL Support](https://www.postgresql.org/docs/current/ssl-tcp.html)
- [Redis TLS Support](https://redis.io/topics/encryption)
- [ClickHouse HTTPS](https://clickhouse.com/docs/en/interfaces/http/)
- [Elasticsearch TLS Configuration](https://www.elastic.co/guide/en/elasticsearch/reference/current/configuring-tls.html)
