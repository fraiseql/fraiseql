# Runbook 15: Tracing / OTLP Export Issues

## Symptoms

- No traces appearing in Jaeger/Tempo/Datadog despite OTLP being configured.
- `Failed to build OTLP exporter` message in stderr at startup.
- High latency spikes correlated with OTLP export timeouts.

## Diagnosis

### 1. Verify OTLP is enabled

Check startup logs for:

```text
OTLP tracing export enabled: endpoint=http://otel-collector:4317, service_name=fraiseql
```

If this line is absent, the endpoint is not configured. Set either:

- `otlp_endpoint` at the top level of the server's configuration file (the file
  `FRAISEQL_CONFIG` names; `fraiseql.toml` does not take it), or
- the `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable.

The file wins: the variable is read only when the file sets no `otlp_endpoint`. This is
the reverse of every other override, so an endpoint set in the environment is ignored
while the file declares one.

### 2. Verify collector reachability

```bash
# From the FraiseQL pod/container
curl -v http://otel-collector:4317/v1/traces
# Should get a response (even if 4xx) — connection refused means collector is unreachable
```

### 3. Check export timeout

If traces are intermittently missing, the default 10-second timeout may be too
short for your collector. Increase it:

```bash
# Environment variable not available — set in config
otlp_export_timeout_secs = 30
```

### 4. Disable OTLP to rule out performance impact

Remove the endpoint, from the configuration file's `otlp_endpoint` as well as
`OTEL_EXPORTER_OTLP_ENDPOINT` (the file's wins), and restart. If latency returns to normal,
the collector is a bottleneck — scale it or use async/batch export (default).

## Resolution

| Problem | Fix |
|---------|-----|
| No traces | Set `OTEL_EXPORTER_OTLP_ENDPOINT` or `otlp_endpoint` |
| Connection refused | Ensure collector is running and reachable |
| Intermittent drops | Increase `otlp_export_timeout_secs` |
| High latency | Scale collector, or check network between app and collector |
| Wrong service name | Set `tracing_service_name` in config |

## Disabling OTLP

To compile without OpenTelemetry entirely (e.g., minimal binary size):

```bash
cargo build --release -p fraiseql-server --no-default-features --features auth
```

This removes all OpenTelemetry dependencies from the binary.
