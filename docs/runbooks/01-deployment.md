# Runbook: Deployment and Verification

## Symptoms

- New FraiseQL server version needs to be deployed
- Current deployment is unhealthy and needs to be restarted
- Compiled schema (schema.compiled.json) needs to be updated
- Rolling back to previous version after failed deployment

## Impact

- Deployment window: Service briefly unavailable (30-60 seconds)
- Configuration changes: May affect all connected clients
- Schema updates: Query compatibility must be verified before deployment
- Rollback: Previous version becomes active, recent data changes may be lost

## Investigation

1. **Check current deployment status**

   ```bash
   # List all FraiseQL containers
   docker ps | grep fraiseql

   # Check server version
   curl http://localhost:8815/health | jq '.version'

   # View deployment logs
   docker logs fraiseql-server --tail 50
   ```

2. **Verify compiled schema is present**

   ```bash
   # Check if schema file exists
   ls -lah /etc/fraiseql/schema.compiled.json

   # Validate schema JSON syntax
   jq empty /etc/fraiseql/schema.compiled.json && echo "Schema valid" || echo "Schema invalid"

   # Check schema version
   jq '.metadata.version' /etc/fraiseql/schema.compiled.json
   ```

3. **Check resource availability**

   ```bash
   # Check disk space
   df -h /etc/fraiseql /var/lib/docker

   # Check available memory
   free -h

   # Check CPU load
   uptime
   ```

4. **Verify database connection**

   ```bash
   # Test database connectivity
   psql $DATABASE_URL -c "SELECT now(), version();" && echo "DB OK" || echo "DB ERROR"

   # Check database size
   psql $DATABASE_URL -c "SELECT pg_database.datname, pg_size_pretty(pg_database_size(pg_database.datname)) FROM pg_database WHERE datname = current_database();"
   ```

5. **Check configuration and secrets**

   ```bash
   # List environment variables
   env | grep -E "^(DB_|REDIS_|VAULT_|RUST_LOG|PORT|SCHEMA_PATH)"

   # Verify Vault connectivity (if using Vault)
   curl -s -H "X-Vault-Token: $VAULT_TOKEN" https://$VAULT_ADDR/v1/auth/token/lookup-self | jq '.data.id'
   ```

## Mitigation

### Option 1: Deploy New Version

```bash
# 1. Pull latest image
docker pull fraiseql:latest

# 2. Stop current container gracefully (allows existing connections to finish)
docker stop -t 30 fraiseql-server

# 3. Backup current container
docker rename fraiseql-server fraiseql-server-backup-$(date +%s)

# 4. Start new container
docker run -d \
  --name fraiseql-server \
  --restart unless-stopped \
  -p 8815:8815 \
  -p 9090:9090 \
  -e DATABASE_URL="$DATABASE_URL" \
  -e REDIS_URL="$REDIS_URL" \
  -e VAULT_ADDR="$VAULT_ADDR" \
  -e VAULT_TOKEN="$VAULT_TOKEN" \
  -e RUST_LOG=info \
  -v /etc/fraiseql:/etc/fraiseql:ro \
  fraiseql:latest

# 5. Wait for server to be ready
sleep 5
curl -v http://localhost:8815/health

# 6. Verify metrics are being generated
curl http://localhost:8815/metrics | head -20
```

### Option 2: Restart Current Version

```bash
# Restart the existing container
docker restart fraiseql-server

# Wait for startup (typically 3-5 seconds)
sleep 5

# Verify it came back online
curl http://localhost:8815/health

# Check logs for any startup errors
docker logs fraiseql-server | tail -20
```

### Option 3: Update Compiled Schema Only

```bash
# 1. Verify new schema syntax
jq empty /etc/fraiseql/schema.compiled.json.new && echo "Schema valid"

# 2. Backup current schema
cp /etc/fraiseql/schema.compiled.json /etc/fraiseql/schema.compiled.json.backup-$(date +%s)

# 3. Deploy new schema
mv /etc/fraiseql/schema.compiled.json.new /etc/fraiseql/schema.compiled.json

# 4. Signal server to reload (send SIGHUP if supported, otherwise restart)
# Most servers need a restart to load new schema
docker restart fraiseql-server

# 5. Verify schema was loaded
sleep 3
curl http://localhost:8815/health | jq '.schema.version'
```

## Resolution

### Complete Deployment Workflow

```bash
#!/bin/bash
set -e

echo "=== FraiseQL Deployment ==="

# Check prerequisites
echo "1. Checking prerequisites..."
if [ -z "$DATABASE_URL" ]; then echo "ERROR: DATABASE_URL not set"; exit 1; fi
if [ ! -f "/etc/fraiseql/schema.compiled.json" ]; then echo "ERROR: schema.compiled.json missing"; exit 1; fi

# Verify schema validity
echo "2. Validating schema..."
jq empty /etc/fraiseql/schema.compiled.json || { echo "ERROR: Invalid schema JSON"; exit 1; }

# Verify database connectivity
echo "3. Testing database..."
psql $DATABASE_URL -c "SELECT 1" > /dev/null || { echo "ERROR: Database unreachable"; exit 1; }

# Backup current state
echo "4. Creating backup..."
docker rename fraiseql-server fraiseql-server-backup-$(date +%s) 2>/dev/null || true

# Deploy
echo "5. Deploying..."
docker pull fraiseql:latest
docker run -d \
  --name fraiseql-server \
  --restart unless-stopped \
  -p 8815:8815 \
  -p 9090:9090 \
  -e DATABASE_URL="$DATABASE_URL" \
  -e REDIS_URL="$REDIS_URL" \
  -e VAULT_ADDR="$VAULT_ADDR" \
  -e VAULT_TOKEN="$VAULT_TOKEN" \
  -e RUST_LOG=info \
  -v /etc/fraiseql:/etc/fraiseql:ro \
  fraiseql:latest

# Wait for startup
echo "6. Waiting for startup..."
for i in {1..30}; do
  if curl -s http://localhost:8815/health > /dev/null; then
    echo "Server online!"
    break
  fi
  echo "  Attempt $i/30..."
  sleep 1
done

# Verify
echo "7. Verification..."
HEALTH=$(curl -s http://localhost:8815/health)
if echo "$HEALTH" | jq -e '.status == "healthy"' > /dev/null; then
  echo "✓ Deployment successful"
  exit 0
else
  echo "✗ Health check failed"
  echo "$HEALTH" | jq .
  exit 1
fi
```

### Rollback to Previous Version

```bash
# 1. Identify previous backup
docker ps -a | grep fraiseql-server-backup

# 2. Stop current version
docker stop fraiseql-server

# 3. Rename current version for recovery
docker rename fraiseql-server fraiseql-server-failed-$(date +%s)

# 4. Restore previous version
docker rename fraiseql-server-backup-<TIMESTAMP> fraiseql-server

# 5. Start
docker start fraiseql-server

# 6. Verify
sleep 3
curl http://localhost:8815/health
```

## Prevention

### Pre-Deployment Checklist

- [ ] Schema compiles without errors: `fraiseql-cli compile schema.json`
- [ ] Schema is validated against database: `fraiseql-cli validate schema.compiled.json`
- [ ] All environment variables are set correctly
- [ ] Database has sufficient space for new migrations (if any)
- [ ] Vault is reachable and has valid credentials
- [ ] Redis is available (if using rate limiting)
- [ ] No other deployments in progress
- [ ] Monitoring and alerting are functioning
- [ ] Backup of previous version exists
- [ ] Deployment window is communicated to stakeholders

### Post-Deployment Verification

```bash
# 1. Check all endpoints are responsive
for endpoint in /health /metrics /graphql; do
  curl -s http://localhost:8815$endpoint > /dev/null && echo "✓ $endpoint" || echo "✗ $endpoint"
done

# 2. Verify database connections
curl -s http://localhost:8815/metrics | grep "db_pool_connections_active"

# 3. Run smoke tests
# Execute key GraphQL queries against test data

# 4. Monitor error rate
curl -s http://localhost:8815/metrics | grep "request_errors_total"

# 5. Check response times
curl -s http://localhost:8815/metrics | grep "request_duration_seconds"
```

## Escalation

- **Deployment issues**: Infrastructure team
- **Schema compilation errors**: Schema/compiler team
- **Database connectivity failures**: Database team (runbook 02)
- **Configuration/secrets issues**: DevOps/Platform team
- **Performance degradation after deployment**: Performance team
- **Critical production outage**: Incident commander
