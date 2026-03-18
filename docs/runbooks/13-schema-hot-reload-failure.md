# Runbook: Schema Hot-Reload Failure

## Symptoms

- Log message: `ERROR Schema reload failed via SIGUSR1 — keeping previous schema`
  or `ERROR Schema reload failed` (admin endpoint)
- Metric: `fraiseql_schema_reload_errors_total` counter incrementing
- Schema-dependent behavior: the server continues serving requests using the
  **previously loaded schema version** — new type definitions, fields, or SQL
  changes are not applied until reload succeeds.
- No query errors for existing operations (the old schema is still valid).
- New operations added in the pending schema return `Field not found` errors.

## Severity

**Medium** — the server is operational but schema changes are not propagating.
Escalate to High if the stale schema persists beyond one reload cycle interval
or if critical security changes (new field authorization rules) are not applied.

---

## Detection

### Metric alert

Configure an alert on `fraiseql_schema_reload_errors_total` rate > 0 for 5 minutes:

```promql
# Alert rule (Prometheus/VictoriaMetrics)
rate(fraiseql_schema_reload_errors_total[5m]) > 0
```

### Log grep

```bash
# Structured logs (JSON)
jq 'select(.message | contains("schema reload failed"))' /var/log/fraiseql/server.log

# Plain-text logs
grep "schema reload failed" /var/log/fraiseql/server.log
```

### Check current schema version

The running server exposes the loaded schema version via the introspection endpoint:

```bash
curl -s http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { description } }"}' | jq .
```

Compare with the expected compiled schema version.

---

## Diagnosis

### Step 1: Find the error message

Hot-reload failures are logged at `ERROR` level with structured fields:

```
ERROR Schema reload failed via SIGUSR1 — keeping previous schema
  error="<error message here>"
  path="/etc/fraiseql/schema.compiled.json"
```

Or from the admin endpoint:

```
ERROR Schema reload failed
  operation="admin.reload_schema"
  schema_path="/etc/fraiseql/schema.compiled.json"
  error="<error message here>"
```

### Step 2: Identify the failure cause

| Error pattern | Likely cause | Resolution |
|---|---|---|
| `Failed to read schema file ... permission denied` | Schema file permissions changed | `chmod 644 schema.compiled.json` |
| `Failed to read schema file ... No such file` | Schema file deleted or moved | Restore or redeploy `schema.compiled.json` |
| `Invalid schema JSON` | Malformed `schema.compiled.json` | Rerun `fraiseql compile` (see Step 3) |
| `Incompatible compiled schema` | Schema compiled with incompatible CLI version | Recompile with matching `fraiseql-cli` |
| `Reload already in progress` | Concurrent reload attempted | Wait for current reload to finish |
| `Reload not configured: no adapter` | Server started without reload config | Restart server (configuration issue) |

### Step 3: Verify the compiled schema

```bash
# Check file exists and is readable
ls -la /etc/fraiseql/schema.compiled.json

# Validate JSON syntax
jq . /etc/fraiseql/schema.compiled.json > /dev/null && echo "JSON valid" || echo "JSON invalid"

# Check schema format version
jq '.schema_format_version' /etc/fraiseql/schema.compiled.json

# Count types, queries, mutations
jq '{types: (.types | length), queries: (.queries | length), mutations: (.mutations | length)}' \
  /etc/fraiseql/schema.compiled.json
```

### Step 4: Recompile if the schema is malformed

```bash
# Regenerate from source
fraiseql generate-schema schema.py > schema.json

# Validate before compiling
fraiseql validate schema.json

# Compile
fraiseql compile schema.json --output /etc/fraiseql/schema.compiled.json

# Verify the output
fraiseql validate /etc/fraiseql/schema.compiled.json
```

---

## Recovery

### Option A: Fix the file and trigger reload

Schema reload is on-demand, not polling-based. Fix the file and then trigger
a reload via SIGUSR1 or the admin endpoint.

Monitor the logs for the success message:

```
INFO Schema reloaded successfully via SIGUSR1
  schema_hash="abc123..."
```

### Option B: Trigger a reload via SIGUSR1

Send `SIGUSR1` to the server process to force an immediate reload attempt:

```bash
# Find the process
pgrep -a fraiseql-server

# Send reload signal
kill -SIGUSR1 $(pgrep fraiseql-server)
```

The server attempts to reload the schema immediately without interrupting
in-flight requests. The reload uses `ArcSwap` for a wait-free atomic pointer
swap — new requests see the new schema instantly while in-flight requests
complete against the old schema.

### Option C: Trigger a reload via admin endpoint

```bash
curl -X POST http://localhost:4000/api/v1/admin/reload-schema \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{"schema_path": "/etc/fraiseql/schema.compiled.json"}'
```

Set `validate_only: true` to validate without applying:

```bash
curl -X POST http://localhost:4000/api/v1/admin/reload-schema \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{"schema_path": "/etc/fraiseql/schema.compiled.json", "validate_only": true}'
```

### Option D: Graceful restart (zero-downtime)

If the process needs to be fully restarted:

```bash
# systemd
sudo systemctl reload fraiseql-server   # SIGUSR1 if supported
sudo systemctl restart fraiseql-server  # Full restart

# Docker
docker exec fraiseql kill -SIGUSR1 1    # Signal PID 1 inside container

# Kubernetes — rolling restart
kubectl rollout restart deployment/fraiseql-server

# Verify rollout
kubectl rollout status deployment/fraiseql-server
```

---

## Reload Scope

Schema reload **only updates the query execution schema** (types, queries,
mutations, SQL templates). The following require a full process restart:

- OIDC validator configuration
- Rate limiter thresholds
- Circuit breaker settings
- Error sanitizer config
- `RequestValidator` settings (`max_query_depth`, `max_complexity`)
- Trusted documents manifest
- API key authenticator
- MCP sessions (existing sessions keep their snapshot; new sessions get the latest)

---

## Prevention

1. **Always validate before deploying:**
   ```bash
   fraiseql validate schema.json && fraiseql compile schema.json
   ```

2. **Add `fraiseql validate` to CI:** the pipeline should reject invalid schemas
   before they reach production. See `.github/workflows/ci.yml` for the
   `fraiseql validate-documents` step.

3. **Alert on reload errors:** configure `fraiseql_schema_reload_errors_total`
   alerting (see Detection section above).

4. **Use atomic file replacement:** avoid writing directly to the live schema
   file; write to a `.tmp` file and `mv` it into place atomically:
   ```bash
   fraiseql compile schema.json --output /etc/fraiseql/schema.compiled.json.tmp
   mv /etc/fraiseql/schema.compiled.json.tmp /etc/fraiseql/schema.compiled.json
   ```

5. **Set correct file permissions:**
   ```bash
   chown fraiseql:fraiseql /etc/fraiseql/schema.compiled.json
   chmod 644 /etc/fraiseql/schema.compiled.json
   ```

---

## Related Runbooks

- [11-schema-migration.md](11-schema-migration.md) — planned schema format migrations
- [02-database-failure.md](02-database-failure.md) — if the new schema queries fail due to DB issues
- [12-incident-response.md](12-incident-response.md) — incident escalation process
