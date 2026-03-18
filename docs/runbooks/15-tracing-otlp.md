# Runbook 15: Tracing / OTLP Export Issues

## Symptoms

- No traces appearing in Jaeger/Tempo/Datadog despite OTLP being configured.
- `Failed to build OTLP exporter` message in stderr at startup.
- High latency spikes correlated with OTLP export timeouts.

## Diagnosis

### 1. Verify OTLP is enabled

Check startup logs for:

```
OTLP tracing export enabled: endpoint=http://otel-collector:4317, service_name=fraiseql
```

If this line is absent, the endpoint is not configured. Set either:

- `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable, or
- `otlp_endpoint` in the server config / `fraiseql.toml`.

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

Remove `OTEL_EXPORTER_OTLP_ENDPOINT` and restart. If latency returns to normal,
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
