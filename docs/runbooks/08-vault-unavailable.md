# Runbook: Vault Unavailable (Secrets Backend Down)

## Symptoms

- Requests failing with `secrets backend unavailable` or `vault connection refused`
- `401 Unauthorized` for OIDC clients (cannot fetch OIDC secrets)
- API keys cannot be validated (stored in Vault)
- Database password cannot be retrieved from Vault
- Field-level encryption keys unavailable
- FraiseQL service cannot start (hangs on secrets initialization)
- Metrics show `vault_connection_errors_total` increasing
- Logs contain: `failed to connect to vault`, `token revoked`, `permission denied`

## Impact

- **Critical**: FraiseQL cannot operate without secrets
- Authentication fails (no keys to validate tokens)
- Database access fails (no password available)
- Field decryption fails (no encryption keys)
- Service must restart once Vault is available

## Investigation

### 1. Vault Connectivity

```bash
# Check Vault address and configuration
env | grep -E "^(VAULT_|SECRET_)"

# Test basic connectivity
VAULT_ADDR="${VAULT_ADDR:-https://vault.example.com:8200}"
echo "Testing Vault at: $VAULT_ADDR"

curl -v -k "$VAULT_ADDR/v1/sys/health" 2>&1 | head -20

# Check DNS resolution
VAULT_HOST=$(echo "$VAULT_ADDR" | cut -d'/' -f3 | cut -d':' -f1)
echo "Resolving $VAULT_HOST:"
nslookup $VAULT_HOST || host $VAULT_HOST

# Check network connectivity
PORT=$(echo "$VAULT_ADDR" | cut -d':' -f3 || echo "8200")
nc -zv $VAULT_HOST $PORT 2>&1

# Check firewall
sudo ufw status | grep -i allow || echo "No ufw rules found"
```

### 2. Vault Health and Status

```bash
# If Vault is reachable, check its health
curl -s "$VAULT_ADDR/v1/sys/health" | jq .

# Possible responses:
# sealed: true       -> Vault is sealed, need to unseal
# standby: true      -> In standby mode (not primary)
# performance_standby: true -> Performance standby
# initialized: false -> Vault not initialized
# code: 473          -> Sealed
# code: 501          -> Not initialized
# code: 200          -> Healthy (unsealed, initialized)

# Check Vault init status
curl -s "$VAULT_ADDR/v1/sys/init" | jq '.'
```

### 3. Vault Authentication

```bash
# Check if token is valid
VAULT_TOKEN="${VAULT_TOKEN:-}"
if [ -z "$VAULT_TOKEN" ]; then
    echo "No VAULT_TOKEN set"
else
    # Verify token
    curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
         "$VAULT_ADDR/v1/auth/token/lookup-self" | jq '.data'

    # Check token expiry
    curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
         "$VAULT_ADDR/v1/auth/token/lookup-self" | jq '.data.expire_time'
fi

# Check if service account credentials are set up
env | grep -E "VAULT_ROLE_ID|VAULT_SECRET_ID"

# If using AppRole auth
curl -s -X POST "$VAULT_ADDR/v1/auth/approle/login" \
     -d "{\"role_id\": \"$VAULT_ROLE_ID\", \"secret_id\": \"$VAULT_SECRET_ID\"}" | jq '.auth.client_token'
```

### 4. FraiseQL Vault Configuration

```bash
# Check how FraiseQL is configured to use Vault
jq '.security' /etc/fraiseql/schema.compiled.json | grep -A 20 "vault\|secrets"

# Check what secrets FraiseQL needs from Vault
jq '.security.vault' /etc/fraiseql/schema.compiled.json

# View vault mount paths and secret types
jq '.security | to_entries[] | select(.value | type == "object")' /etc/fraiseql/schema.compiled.json
```

### 5. Required Secrets in Vault

```bash
# Check what paths FraiseQL is trying to read
docker logs fraiseql-server | grep -i "vault\|secret" | tail -30

# If token has access, try reading the paths
VAULT_TOKEN="$VAULT_TOKEN"
VAULT_ADDR="$VAULT_ADDR"

# Try reading database credentials
curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
     "$VAULT_ADDR/v1/secret/data/fraiseql/database" | jq '.data.data'

# Try reading JWT secrets
curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
     "$VAULT_ADDR/v1/secret/data/fraiseql/jwt" | jq '.data.data'

# Try reading API key vault
curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
     "$VAULT_ADDR/v1/secret/data/fraiseql/api_keys" | jq '.data.data'
```

### 6. Vault Storage Issues

```bash
# Check Vault storage backend status
curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
     "$VAULT_ADDR/v1/sys/storage" | jq '.data'

# Check if storage is accessible (disk space, etc.)
curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
     "$VAULT_ADDR/v1/sys/storage/raft/configuration" | jq '.data' 2>/dev/null || echo "Using different storage backend"
```

## Mitigation

### Immediate (< 5 minutes)

1. **Check if Vault is just restarting**
   ```bash
   # Wait for it to come online
   while ! curl -s "$VAULT_ADDR/v1/sys/health" > /dev/null; do
       echo "Waiting for Vault..."
       sleep 5
   done
   echo "Vault is back online"

   # Check if sealed and unseal if needed
   SEALED=$(curl -s "$VAULT_ADDR/v1/sys/health" | jq '.sealed')
   if [ "$SEALED" = "true" ]; then
       echo "Vault is sealed, attempting unseal..."
       # Use unseal keys (must have them stored securely)
       # See organization's Vault unseal procedure
   fi

   # Restart FraiseQL once Vault is ready
   docker restart fraiseql-server
   ```

2. **Use fallback/cached credentials** (if available)
   ```bash
   # If Vault is down but data was cached
   export FRAISEQL_VAULT_FALLBACK_MODE=cached
   export VAULT_ADDR=""  # Disable Vault checks
   docker restart fraiseql-server

   # This allows service to run with cached credentials temporarily
   # But authentication/encryption may be limited
   ```

3. **Check Vault logs for errors**
   ```bash
   # If running self-hosted Vault
   docker logs vault | grep -i "error\|fatal" | tail -20

   # Check Vault metrics
   curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
        "$VAULT_ADDR/v1/sys/metrics" | jq '.data.gauges' | head -20
   ```

### Short-term (5-30 minutes)

4. **Verify Vault is initialized and unsealed**
   ```bash
   # Check initialization
   STATUS=$(curl -s "$VAULT_ADDR/v1/sys/health")
   echo "Vault health:"
   echo "$STATUS" | jq '{initialized: .initialized, sealed: .sealed, version: .version}'

   # If sealed, unseal using backup unseal keys
   # Procedure varies by organization - contact Vault admin

   # If not initialized, run initialization (backup access to unseal keys!)
   # curl -X POST "$VAULT_ADDR/v1/sys/init" \
   #   -d '{"secret_shares": 5, "secret_threshold": 3}'
   ```

5. **Verify token is valid**
   ```bash
   # Check if token is revoked or expired
   LOOKUP=$(curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
            "$VAULT_ADDR/v1/auth/token/lookup-self")

   echo "$LOOKUP" | jq '.errors // .data | {ttl: .ttl, expire_time: .expire_time, policies: .policies}'

   # If expired, get new token
   # Method depends on auth mechanism (AppRole, JWT, LDAP, etc.)

   # For AppRole:
   NEW_TOKEN=$(curl -s -X POST "$VAULT_ADDR/v1/auth/approle/login" \
                     -d "{\"role_id\": \"$VAULT_ROLE_ID\", \"secret_id\": \"$VAULT_SECRET_ID\"}" | jq -r '.auth.client_token')

   export VAULT_TOKEN="$NEW_TOKEN"
   docker restart fraiseql-server
   ```

6. **Check and fix network connectivity**
   ```bash
   # If DNS fails
   systemctl restart systemd-resolved
   # or
   systemctl restart resolvconf

   # If firewall blocking
   sudo ufw allow from any to any port 8200  # Vault port
   sudo ufw reload

   # If routing issue
   sudo ip route add default via <gateway>

   # Re-test connectivity
   curl -v "$VAULT_ADDR/v1/sys/health"
   ```

### Extended Outage (30+ minutes)

7. **Disable Vault requirement temporarily**
   ```bash
   # Last resort: Run without Vault (limited functionality)
   # Set credentials directly in environment

   export DATABASE_PASSWORD="fallback-password"  # Must have backup
   export JWT_SECRET="fallback-jwt-key"
   # Note: This bypasses secret management, high security risk!

   export FRAISEQL_VAULT_REQUIRED=false
   docker restart fraiseql-server
   ```

8. **Switch to backup Vault instance**
   ```bash
   # If you have a replicated/backup Vault
   export VAULT_ADDR="https://vault-backup.example.com:8200"
   export VAULT_TOKEN="backup-vault-token"

   docker restart fraiseql-server
   ```

## Resolution

### Complete Vault Recovery Workflow

```bash
#!/bin/bash
set -e

echo "=== Vault Recovery ==="

VAULT_ADDR="${VAULT_ADDR:-https://vault.example.com:8200}"

# 1. Ping Vault
echo "1. Checking Vault connectivity..."
if curl -s "$VAULT_ADDR/v1/sys/health" > /dev/null 2>&1; then
    echo "   ✓ Vault is reachable"
else
    echo "   ✗ Vault is unreachable"
    echo "   Checking DNS..."
    nslookup $(echo "$VAULT_ADDR" | cut -d'/' -f3 | cut -d':' -f1)
    exit 1
fi

# 2. Check initialization and seal status
echo ""
echo "2. Checking Vault status..."
STATUS=$(curl -s "$VAULT_ADDR/v1/sys/health")
INITIALIZED=$(echo "$STATUS" | jq '.initialized')
SEALED=$(echo "$STATUS" | jq '.sealed')
echo "   Initialized: $INITIALIZED"
echo "   Sealed: $SEALED"

if [ "$SEALED" = "true" ]; then
    echo "   ✗ Vault is sealed - cannot proceed"
    echo "   Contact Vault administrator for unseal keys"
    exit 1
fi

if [ "$INITIALIZED" = "false" ]; then
    echo "   ✗ Vault is not initialized"
    exit 1
fi

# 3. Verify authentication
echo ""
echo "3. Verifying token..."
if [ -z "$VAULT_TOKEN" ]; then
    echo "   ✗ No VAULT_TOKEN set"
    exit 1
fi

TOKEN_LOOKUP=$(curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
                    "$VAULT_ADDR/v1/auth/token/lookup-self")

if echo "$TOKEN_LOOKUP" | jq -e '.errors' > /dev/null; then
    echo "   ✗ Token is invalid"
    echo "   Errors: $(echo "$TOKEN_LOOKUP" | jq '.errors')"
    exit 1
fi

TTL=$(echo "$TOKEN_LOOKUP" | jq '.data.ttl')
echo "   ✓ Token valid (TTL: ${TTL}s)"

# 4. Verify required secrets exist
echo ""
echo "4. Checking required secrets..."

SECRETS_OK=true

# Check database credentials
if curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
        "$VAULT_ADDR/v1/secret/data/fraiseql/database" | jq -e '.data.data.password' > /dev/null; then
    echo "   ✓ Database credentials accessible"
else
    echo "   ✗ Database credentials not found"
    SECRETS_OK=false
fi

# Check JWT secrets
if curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
        "$VAULT_ADDR/v1/secret/data/fraiseql/jwt" | jq -e '.data.data.secret' > /dev/null; then
    echo "   ✓ JWT secrets accessible"
else
    echo "   ✗ JWT secrets not found"
    SECRETS_OK=false
fi

if [ "$SECRETS_OK" = "false" ]; then
    echo ""
    echo "   Required secrets missing in Vault"
    exit 1
fi

# 5. Restart FraiseQL
echo ""
echo "5. Restarting FraiseQL..."
docker restart fraiseql-server
sleep 5

# 6. Verify FraiseQL is online
echo ""
echo "6. Verifying FraiseQL..."
if curl -s http://localhost:8815/health | jq -e '.status == "healthy"' > /dev/null; then
    echo "   ✓ FraiseQL is healthy"
    exit 0
else
    echo "   ✗ FraiseQL failed to start"
    docker logs fraiseql-server | grep -i "vault\|error" | tail -10
    exit 1
fi
```

### Vault Initialization (if needed)

```bash
# This is a sensitive operation - coordinate with security team

# 1. Initialize Vault (if first time)
curl -X POST "$VAULT_ADDR/v1/sys/init" \
  -d '{
    "secret_shares": 5,
    "secret_threshold": 3,
    "pgp_keys": ["<base64-encoded-pgp-keys...>"]
  }' | jq .

# BACKUP the unseal keys and root token immediately
# Store in secure location (HSM, KMS, etc.)

# 2. Unseal Vault with 3 of the 5 keys
curl -X POST "$VAULT_ADDR/v1/sys/unseal" \
  -d '{"key": "unseal_key_1_data"}'

curl -X POST "$VAULT_ADDR/v1/sys/unseal" \
  -d '{"key": "unseal_key_2_data"}'

curl -X POST "$VAULT_ADDR/v1/sys/unseal" \
  -d '{"key": "unseal_key_3_data"}'

# 3. Create service account for FraiseQL
curl -s -H "X-Vault-Token: $ROOT_TOKEN" \
     -X POST "$VAULT_ADDR/v1/auth/approle/role/fraiseql" \
     -d '{"token_ttl": "3600"}'

# 4. Get role_id and secret_id
ROLE_ID=$(curl -s -H "X-Vault-Token: $ROOT_TOKEN" \
               "$VAULT_ADDR/v1/auth/approle/role/fraiseql/role-id" | jq -r '.data.role_id')

SECRET_ID=$(curl -s -H "X-Vault-Token: $ROOT_TOKEN" \
                 -X POST "$VAULT_ADDR/v1/auth/approle/role/fraiseql/secret-id" | jq -r '.data.secret_id')

# 5. Use these for FraiseQL authentication
export VAULT_ROLE_ID="$ROLE_ID"
export VAULT_SECRET_ID="$SECRET_ID"
```

## Prevention

### Monitoring and Alerting

```bash
# Prometheus alerts for Vault
cat > /etc/prometheus/rules/fraiseql-vault.yml << 'EOF'
groups:
  - name: fraiseql_vault
    rules:
      - alert: VaultUnavailable
        expr: vault_health == 0
        for: 1m
        action: page

      - alert: VaultSealed
        expr: vault_sealed == 1
        for: 1m
        action: page

      - alert: VaultTokenExpiring
        expr: (vault_token_ttl < 3600)
        for: 15m
        action: notify

      - alert: FraiseQLVaultConnectionErrors
        expr: rate(vault_connection_errors_total[5m]) > 0
        for: 5m
        action: notify
EOF
```

### Best Practices

- **Backup unseal keys**: Store in multiple secure locations (HSM, KMS, safe deposit box)
- **Monitor Vault uptime**: Alert on any unavailability
- **Rotate tokens regularly**: Don't use tokens with long TTLs
- **Use AppRole for FraiseQL**: More secure than root tokens
- **Enable audit logging**: Log all secret access for compliance
- **Test recovery**: Regularly practice Vault unsealing procedures
- **High availability**: Deploy Vault in HA mode with replication

### Maintenance Schedule

```bash
# Weekly: Verify Vault health
curl -s "$VAULT_ADDR/v1/sys/health" | jq '.'

# Weekly: Check FraiseQL can access Vault
curl -s http://localhost:8815/health | jq '.vault_connected'

# Monthly: Rotate service account credentials
# curl -X POST "$VAULT_ADDR/v1/auth/approle/role/fraiseql/secret-id"

# Monthly: Review Vault audit logs
curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
     "$VAULT_ADDR/v1/sys/audit"

# Quarterly: Verify backup unseal keys are still valid
# Test unsealing in test/dev environment

# Annually: Security audit of Vault policies and access
```

## Escalation

- **Vault not responding**: Infrastructure / Vault administrator
- **Vault sealed and no unseal keys**: Vault administrator + Security
- **Token expired/invalid**: Security / DevOps team
- **Vault storage issues**: Infrastructure / Database team
- **Replication lag (if HA)**: Vault administrator
- **Requires manual unseal**: Vault disaster recovery team
