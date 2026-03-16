# Runbook: Schema Hot-Reload Failure

## Symptoms

- Log message: `WARN fraiseql_server::schema::hot_reload: schema reload failed`
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

Hot-reload failures log the underlying error at `WARN` level with structured fields:

```
WARN fraiseql_server::schema::hot_reload: schema reload failed
  error="<error message here>"
  path="/etc/fraiseql/schema.compiled.json"
  attempt=3
```

### Step 2: Identify the failure cause

| Error pattern | Likely cause | Resolution |
|---|---|---|
| `io error: permission denied` | Schema file permissions changed | `chmod 644 schema.compiled.json` |
| `io error: no such file or directory` | Schema file deleted or moved | Restore or redeploy `schema.compiled.json` |
| `JSON parse error` | Malformed `schema.compiled.json` | Rerun `fraiseql compile` (see Step 3) |
| `Schema format version mismatch` | Schema compiled with incompatible CLI version | Recompile with matching `fraiseql-cli` |
| `validation error: unknown type` | Schema references a type not yet defined | Fix the schema definition and recompile |
| `timeout` | Disk I/O timeout or NFS stall | Check filesystem health (`df -h`, `dmesg`) |
| `connection refused` | Remote schema source unavailable | Check network connectivity to schema source |

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

### Option A: Fix the file and wait for the next reload cycle

The hot-reload interval defaults to 30 seconds (configurable via
`[hot_reload] interval_secs` in `fraiseql.toml`). Once the file is corrected,
the next reload cycle picks it up automatically.

```toml
# fraiseql.toml
[hot_reload]
interval_secs = 30        # How often to check for schema changes
enabled = true
```

Monitor the logs for the success message:

```
INFO fraiseql_server::schema::hot_reload: schema reloaded successfully
  version=1
  types=12
  queries=8
  mutations=5
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
in-flight requests.

### Option C: Graceful restart (zero-downtime)

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
