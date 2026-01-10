# Phase 02: Add Loki Configuration Examples

**Priority:** MEDIUM
**Time Estimate:** 1.5 hours
**Impact:** +1.0 point to Observability score (13/15 → 14/15)
**Status:** ⬜ Not Started

---

## Problem Statement

Pentagon-Readiness Assessment notes "Limited Loki implementation evidence" (-1 point). While OpenTelemetry and Prometheus are documented, Loki (log aggregation) configuration is missing. We need production-ready Loki configuration examples and integration documentation.

---

## Objective

Create Loki configuration examples that enable operators to:
1. Deploy Loki + Promtail for log aggregation
2. Parse FraiseQL application logs
3. Integrate with existing Grafana dashboards
4. Query logs with LogQL for troubleshooting

**Deliverables:**
- Loki server configuration
- Promtail agent configuration
- Docker Compose setup for quick deployment
- Integration guide with query examples

---

## Context Files

**Review these files before writing (orchestrator will copy to `context/`):**
- `docs/production/MONITORING.md` - Existing observability setup
- `examples/observability/` - Existing observability examples (if any)
- Any existing docker-compose files
- Log format documentation (if exists)

**External References:**
- Loki documentation: https://grafana.com/docs/loki/latest/
- Promtail configuration: https://grafana.com/docs/loki/latest/clients/promtail/configuration/
- LogQL query language: https://grafana.com/docs/loki/latest/logql/

---

## Deliverables

### 1. Loki Server Configuration

**File:** `.phases/02-loki-configuration/output/loki-config.yaml`

**Requirements:**
- [ ] Basic server configuration (HTTP port 3100)
- [ ] Retention policy (30 days for production)
- [ ] Storage backend: filesystem for dev/testing, S3/GCS for production
- [ ] Schema configuration for indexing
- [ ] Chunk storage configuration
- [ ] Limits configuration (reasonable defaults)

**Key Sections:**
```yaml
server:
  http_listen_port: 3100

ingester:
  # Chunk configuration

schema_config:
  # Index schema

storage_config:
  # Local filesystem (dev) and cloud storage (prod) options

limits_config:
  # Retention period: 720h (30 days)
  # Query limits

table_manager:
  # Retention enforcement
```

---

### 2. Promtail Agent Configuration

**File:** `.phases/02-loki-configuration/output/promtail-config.yaml`

**Requirements:**
- [ ] Scrape configuration for FraiseQL logs
- [ ] JSON log parsing (if FraiseQL logs are JSON)
- [ ] Label extraction (level, trace_id, span_id)
- [ ] Integration with OpenTelemetry trace IDs
- [ ] Multiple log sources (application, audit, errors)

**Key Sections:**
```yaml
server:
  http_listen_port: 9080

clients:
  - url: http://loki:3100/loki/api/v1/push

scrape_configs:
  - job_name: fraiseql-app
    # Application logs

  - job_name: fraiseql-audit
    # Audit logs (if separate)

  - job_name: fraiseql-errors
    # Error logs (if separate)

pipeline_stages:
  # JSON parsing
  # Label extraction
  # Timestamp parsing
```

**Important:** Extract trace_id and span_id from logs for correlation with OpenTelemetry traces.

---

### 3. Docker Compose Setup

**File:** `.phases/02-loki-configuration/output/docker-compose.loki.yml`

**Requirements:**
- [ ] Loki service with volume mounts
- [ ] Promtail service with log volume mounts
- [ ] Grafana service (optional, for testing)
- [ ] Network configuration
- [ ] Volume definitions
- [ ] Health checks

**Services:**
```yaml
version: '3.8'

services:
  loki:
    # Loki service definition

  promtail:
    # Promtail service definition
    # Mount /var/log and docker container logs

  grafana:
    # Optional: Grafana for testing
    # Pre-configure Loki as data source

volumes:
  loki-data:
  grafana-data:
```

---

### 4. Integration Guide

**File:** `.phases/02-loki-configuration/output/LOKI_INTEGRATION.md`

**Requirements:**
- [ ] Overview of Loki + Promtail architecture
- [ ] Installation instructions (Docker Compose)
- [ ] Configuration for development vs production
- [ ] How to add Loki to Grafana as data source
- [ ] At least 5 common LogQL query examples
- [ ] Troubleshooting section

**Required Sections:**

#### Architecture Overview
- Loki: Log aggregation and storage
- Promtail: Log collector agent
- Grafana: Query and visualization
- Integration with OpenTelemetry traces

#### Quick Start (Docker Compose)
```bash
# Start Loki stack
docker-compose -f examples/observability/docker-compose.loki.yml up -d

# Verify Loki is running
curl http://localhost:3100/ready

# Add to Grafana data sources
# URL: http://loki:3100
```

#### Production Configuration
- Use S3/GCS for storage
- Scale ingester and querier components
- Configure retention policies
- Set up authentication (if required)

#### Common LogQL Queries

**Must include examples for:**
1. **All errors in last hour:**
   ```logql
   {job="fraiseql-app"} |= "ERROR" [1h]
   ```

2. **Logs for specific trace:**
   ```logql
   {job="fraiseql-app"} | json | trace_id="abc123"
   ```

3. **Rate of errors per minute:**
   ```logql
   rate({job="fraiseql-app"} |= "ERROR" [5m])
   ```

4. **Top 10 error messages:**
   ```logql
   topk(10, sum by (message) (rate({job="fraiseql-app"} | json | level="error" [1h])))
   ```

5. **Filter by user or tenant:**
   ```logql
   {job="fraiseql-app"} | json | user_id="user123"
   ```

#### Correlation with OpenTelemetry
- How to jump from Loki log to Tempo trace using trace_id
- How to configure Grafana to link logs and traces
- Example of unified observability workflow

#### Troubleshooting
- Loki not receiving logs
- Promtail parsing errors
- Storage issues
- Query performance

---

## Directory Structure

After completion:

```
.phases/02-loki-configuration/output/
├── loki-config.yaml
├── promtail-config.yaml
├── docker-compose.loki.yml
└── LOKI_INTEGRATION.md
```

These will be moved to:
```
examples/observability/loki/
├── loki-config.yaml
├── promtail-config.yaml
└── docker-compose.loki.yml

docs/production/
└── LOKI_INTEGRATION.md
```

---

## Configuration Examples Reference

### Log Format Assumptions

FraiseQL likely logs in JSON format with fields like:
```json
{
  "timestamp": "2025-12-04T10:15:30Z",
  "level": "error",
  "message": "Database connection failed",
  "trace_id": "abc123...",
  "span_id": "def456...",
  "user_id": "user789",
  "tenant_id": "tenant123"
}
```

Adjust Promtail pipeline based on actual log format found in context files.

---

## Verification (Orchestrator)

After junior engineer delivers configuration files:

```bash
# 1. Validate YAML syntax
uv run python -c "import yaml; yaml.safe_load(open('.phases/02-loki-configuration/output/loki-config.yaml'))"
uv run python -c "import yaml; yaml.safe_load(open('.phases/02-loki-configuration/output/promtail-config.yaml'))"
uv run python -c "import yaml; yaml.safe_load(open('.phases/02-loki-configuration/output/docker-compose.loki.yml'))"

# 2. Check docker-compose config
cd .phases/02-loki-configuration/output/
docker-compose -f docker-compose.loki.yml config

# 3. Verify required services present
grep -E "(loki:|promtail:)" docker-compose.loki.yml

# 4. Check integration guide has LogQL examples
grep -c "logql" LOKI_INTEGRATION.md
# Should have at least 5 examples

# 5. Verify trace_id extraction in promtail config
grep "trace_id" promtail-config.yaml
```

**Optional:** If time permits, test the stack:
```bash
# Start stack
docker-compose -f docker-compose.loki.yml up -d

# Check Loki health
curl http://localhost:3100/ready

# Send test log
curl -X POST http://localhost:3100/loki/api/v1/push \
  -H "Content-Type: application/json" \
  -d '{"streams":[{"stream":{"job":"test"},"values":[["'$(date +%s)000000000'","test log"]]}]}'

# Query logs
curl -G http://localhost:3100/loki/api/v1/query \
  --data-urlencode 'query={job="test"}'

# Cleanup
docker-compose -f docker-compose.loki.yml down -v
```

---

## Final Placement (Orchestrator)

After verification passes:

```bash
# Create directories
mkdir -p examples/observability/loki
mkdir -p docs/production

# Move configuration files
cp .phases/02-loki-configuration/output/loki-config.yaml examples/observability/loki/
cp .phases/02-loki-configuration/output/promtail-config.yaml examples/observability/loki/
cp .phases/02-loki-configuration/output/docker-compose.loki.yml examples/observability/

# Move documentation
cp .phases/02-loki-configuration/output/LOKI_INTEGRATION.md docs/production/

# Commit
git add examples/observability/loki/ examples/observability/docker-compose.loki.yml docs/production/LOKI_INTEGRATION.md
git commit -m "feat(observability): add Loki log aggregation configuration

Add production-ready Loki configuration and integration guide:
- Loki server configuration with 30-day retention
- Promtail agent configuration with JSON parsing
- Docker Compose setup for quick deployment
- Integration guide with 5+ LogQL query examples
- OpenTelemetry trace correlation support

Impact: +1 point to Observability score (13/15 → 14/15)

Refs: Pentagon-Readiness Assessment - Phase 02"
```

---

## Tips for Documentation Writer

1. **Review observability context:** Understand existing OpenTelemetry and Prometheus setup
2. **Use production-ready defaults:** Don't use "localhost" or "changeme" passwords
3. **Label extraction is key:** Make sure trace_id, span_id, level are extracted as labels
4. **LogQL examples should be realistic:** Use actual field names from FraiseQL logs
5. **Storage backend matters:** Document both filesystem (dev) and S3/GCS (prod) options
6. **Test configurations mentally:** Would this work in production? Is retention appropriate?

---

## Success Criteria

- [ ] File created: `loki-config.yaml` with valid YAML syntax
- [ ] File created: `promtail-config.yaml` with JSON parsing and label extraction
- [ ] File created: `docker-compose.loki.yml` with all required services
- [ ] File created: `LOKI_INTEGRATION.md` with 5+ LogQL query examples
- [ ] All YAML files validate successfully
- [ ] Docker Compose config is valid
- [ ] Integration guide includes OpenTelemetry correlation instructions
- [ ] Documentation is clear and actionable
