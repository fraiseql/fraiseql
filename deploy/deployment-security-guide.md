# FraiseQL Deployment Security Guide

**Applies to**: FraiseQL 2.15.0
**Last updated**: 2026-08-30

> This guide described a **Python** deployment until 2026-08-30: a `python:3.13-slim`
> base, a `deploy/docker/Dockerfile.hardened` that is not in the repository, and a
> `fraiseql-server:local` image that was never published anywhere (a bare name resolves
> to `docker.io/library/fraiseql`, the Docker Hub official-images namespace, which this
> project cannot publish to). FraiseQL v2 is Rust. The sections below that name an
> artifact now name one that exists; the ones that describe posture rather than artifacts
> — network policy, secrets, compliance mappings, audit-log integrity — were already
> product-agnostic and are unchanged (#1220).

## Table of Contents

1. [Overview](#overview)
2. [Quick Start](#quick-start)
3. [Security Architecture](#security-architecture)
4. [Deployment Options](#deployment-options)
5. [Configuration](#configuration)
6. [Monitoring & Alerting](#monitoring--alerting)
7. [Compliance](#compliance)
8. [Troubleshooting](#troubleshooting)

---

## Overview

### What is actually shipped

**Runtime image**: `debian:bookworm-slim`, carrying one statically-configured Rust binary.
The builder stage is `rust:1.94.1-slim`; nothing from it reaches the runtime image.

**Published as** (`.github/workflows/docker-build.yml`, on `v*` tags):

- `ghcr.io/fraiseql/server` — the default feature set
- `ghcr.io/fraiseql/server-full` — plus `rest-transport` and `arrow`
- `fraiseql/server` on Docker Hub

**Compliance**: NIST 800-53, NIS2, ISO 27001, FedRAMP Moderate — see [Compliance](#compliance).

### Why `debian:bookworm-slim` and not distroless

The binary's only dynamic dependencies are `libc`, `libm` and `libgcc_s`, so distroless
is reachable. `bookworm-slim` is what ships today because it keeps a shell for incident
response and because nothing in the image links a system library that would make the
difference material — the PostgreSQL driver is the pure-Rust `tokio-postgres` + rustls
stack, and `libpq` appears nowhere in `Cargo.lock`. The runtime image installs **no**
packages beyond the base (#1133).

---

## Quick Start

### Build the image

There is one Dockerfile, at the repository root. It is the file the release workflow
builds and the one `tools/compose-stack-test.sh` exercises on the `Dagger — image` leg.

```bash
# Build the runtime image from the current tree
docker build --tag fraiseql-server:local .

# Scan it
docker run --rm -v /var/run/docker.sock:/var/run/docker.sock \
  aquasec/trivy:latest image \
  --severity HIGH,CRITICAL \
  fraiseql-server:local
```

Or pull a published one instead of building:

```bash
docker pull ghcr.io/fraiseql/server:2.15.0
```

### Deploy to Kubernetes

```bash
# Apply all security configurations
kubectl apply -f deploy/kubernetes/fraiseql-hardened.yaml

# Verify deployment
kubectl get pods -n fraiseql-production
kubectl describe pod -n fraiseql-production -l app=fraiseql

# Check security context
kubectl get pod -n fraiseql-production -l app=fraiseql -o jsonpath='{.items[0].spec.securityContext}' | jq
```

---

## Security Architecture

### Defense-in-Depth Layers

```
┌─────────────────────────────────────────────────────────────┐
│                     Network Layer                            │
│  • Ingress TLS termination                                   │
│  • Network policies (zero-trust)                             │
│  • Rate limiting, DDoS protection                            │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                  Application Layer                           │
│  • Input validation                                          │
│  • CSRF protection                                           │
│  • GraphQL query complexity limits                           │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                   Container Layer                            │
│  • debian:bookworm-slim, one Rust binary, no added packages   │
│  • Non-root user (UID 65532)                                 │
│  • Read-only root filesystem                                 │
│  • No shell, minimal packages                                │
│  • Drop all capabilities                                     │
│  • Seccomp: RuntimeDefault                                   │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                Infrastructure Layer                          │
│  • Encrypted storage (at rest)                               │
│  • mTLS service mesh                                         │
│  • Secrets management (Vault/KMS)                            │
│  • Audit logging                                             │
└─────────────────────────────────────────────────────────────┘
```

### Security Features

#### Container Hardening

- ✅ Non-root user (UID 65532)
- ✅ Read-only root filesystem
- ✅ Dropped all Linux capabilities
- ✅ No privilege escalation
- ✅ Seccomp profile: RuntimeDefault
- ✅ AppArmor/SELinux compatible

#### Network Security

- ✅ Network policies (zero-trust)
- ✅ Ingress TLS with strong ciphers
- ✅ mTLS for service-to-service (recommended)
- ✅ Rate limiting and connection limits
- ✅ Egress filtering

#### Runtime Security

- ✅ Falco for threat detection
- ✅ Unauthorized process detection
- ✅ File integrity monitoring
- ✅ Network anomaly detection
- ✅ Automated alerting

---

## Deployment Options

### Option 1: Kubernetes (Recommended)

**Best for**: Production, government agencies, high-security environments

```bash
# Deploy with full security stack
kubectl apply -f deploy/kubernetes/fraiseql-hardened.yaml

# Install Falco for runtime security
helm repo add falcosecurity https://falcosecurity.github.io/charts
helm install falco falcosecurity/falco \
  --namespace falco --create-namespace \
  --set-file customRules.fraiseql=deploy/security/falco-rules.yaml
```

**Security features**:

- Pod Security Standards: restricted
- Network policies (zero-trust)
- Read-only root filesystem
- Resource limits
- Horizontal Pod Autoscaler
- Pod Disruption Budget
- Secrets management

### Option 2: Docker Compose

**Best for**: Development, staging, small deployments

Use the repository's root `docker-compose.yml`. It is the **one** Compose stack this
project verifies: `tools/compose-stack-test.sh` brings it up on the image the current
branch builds, waits for the container's own HEALTHCHECK, queries it through the
published port, and then inserts a row behind the engine's back and requires that row in
the answer. It runs as a step on the `Dagger — image` leg.

```bash
docker compose up -d
docker compose ps          # the server must reach `healthy`, not just `running`
```

There is deliberately no second, inline copy of the stack here. This section used to
carry one, pinning `fraiseql-server:local` — an image that has never existed — and
setting `DATABASE_URL` and nothing else, so a container started from it exited on
`cors_enabled is true but cors_origins is empty in production mode` before it ever
reached the missing schema. Five other operator-facing stacks were deleted for the same
class of defect on 2026-08-28; see the header comment of `docker-compose.yml` for what
each of them was doing wrong.

For the hardening that snippet was trying to show — read-only root filesystem, dropped
capabilities, `no-new-privileges`, non-root UID, seccomp — see
`deploy/kubernetes/fraiseql-hardened.yaml`, which is rendered and checked by
`tools/chart-deploy-test.sh` rather than pasted into prose.

### Option 3: Cloud Services

#### AWS ECS/Fargate

```json
{
  "family": "fraiseql",
  "containerDefinitions": [
    {
      "name": "fraiseql",
      "image": "ghcr.io/fraiseql/server:2.15.0",
      "user": "65532",
      "readonlyRootFilesystem": true,
      "linuxParameters": {
        "capabilities": {
          "drop": ["ALL"]
        }
      },
      "secrets": [
        {
          "name": "DATABASE_URL",
          "valueFrom": "arn:aws:secretsmanager:region:account:secret:fraiseql-db-url"
        }
      ]
    }
  ]
}
```

#### Google Cloud Run

```bash
gcloud run deploy fraiseql \
  --image=ghcr.io/fraiseql/server:2.15.0 \
  --platform=managed \
  --region=us-central1 \
  --no-allow-unauthenticated \
  --service-account=fraiseql@your-project.iam.gserviceaccount.com \
  --set-secrets=DATABASE_URL=fraiseql-db-url:latest \
  --execution-environment=gen2 \
  --no-cpu-throttling \
  --min-instances=1 \
  --max-instances=10
```

---

## Configuration

### Environment Variables

```bash
# Required
DATABASE_URL=postgresql://user:pass@host:5432/db

# Optional
FRAISEQL_PRODUCTION=true
LOG_LEVEL=INFO
WORKERS=4
MAX_CONNECTIONS=100

# Observability
OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4318
PROMETHEUS_MULTIPROC_DIR=/tmp/prometheus
```

### Secrets Management

**DO NOT** hardcode secrets in configuration files.

#### Option 1: Kubernetes Secrets + External Secrets Operator

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: fraiseql-secrets
  namespace: fraiseql-production
spec:
  secretStoreRef:
    name: vault-backend
    kind: SecretStore
  target:
    name: fraiseql-secrets
  data:
  - secretKey: DATABASE_URL
    remoteRef:
      key: fraiseql/production
      property: database_url
```

#### Option 2: AWS Secrets Manager

```bash
# Store secret
aws secretsmanager create-secret \
  --name fraiseql/database-url \
  --secret-string "postgresql://user:pass@host:5432/db"

# Reference in ECS task definition
"secrets": [
  {
    "name": "DATABASE_URL",
    "valueFrom": "arn:aws:secretsmanager:region:account:secret:fraiseql/database-url"
  }
]
```

#### Option 3: HashiCorp Vault

```bash
# Write secret
vault kv put secret/fraiseql/production \
  database_url="postgresql://user:pass@host:5432/db"

# Inject via Vault Agent
vault agent -config=vault-agent-config.hcl
```

---

## Monitoring & Alerting

### Metrics (Prometheus)

```yaml
# Prometheus scrape config
scrape_configs:
  - job_name: 'fraiseql'
    kubernetes_sd_configs:
      - role: pod
        namespaces:
          names:
            - fraiseql-production
    relabel_configs:
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_scrape]
        action: keep
        regex: true
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_path]
        action: replace
        target_label: __metrics_path__
        regex: (.+)
```

**Key Metrics**:

- `fraiseql_requests_total` - Total HTTP requests
- `fraiseql_request_duration_seconds` - Request latency
- `fraiseql_graphql_queries_total` - GraphQL queries
- `fraiseql_database_connections` - Active DB connections
- `fraiseql_errors_total` - Application errors

### Logs (Structured JSON)

```python
# Application logging configuration
{
  "timestamp": "2025-12-09T15:45:00Z",
  "level": "INFO",
  "logger": "fraiseql.api",
  "message": "GraphQL query executed",
  "query": "{ users { id name } }",
  "duration_ms": 42,
  "user_id": "user-123",
  "trace_id": "abc123def456"
}
```

### Security Alerts (Falco)

Falco rules monitor for:

- ✅ Unexpected processes
- ✅ Shell execution
- ✅ Unauthorized file writes
- ✅ Privilege escalation attempts
- ✅ Crypto mining activity
- ✅ Package manager execution
- ✅ Sensitive file access

**Alert Destinations**:

- Slack
- PagerDuty
- Email
- Prometheus metrics
- SIEM (Splunk, ELK)

### Health Checks

```bash
# Liveness — is the PROCESS alive? Always 200, no dependency call (#1217)
curl -f http://localhost:8000/live

# Readiness — ready for traffic? 503 while the database is unreachable
curl -f http://localhost:8000/readiness

# Startup — before a pod has served, an unreachable database IS a startup failure
curl -f http://localhost:8000/health

# Operator status — the full subsystem report; 200 healthy or degraded, 503 when the
# database is down. Do NOT probe liveness with this: it restarts every pod through a
# database outage, which restarting cannot fix.
curl -s http://localhost:8000/health | jq
```

The path was `/ready` in this guide until 2026-08-30. There is no `/ready` endpoint;
`curl -f` against it answers 404, and a probe configured from this line would have failed
every check.

---

## Compliance

### NIST 800-53 Controls

| Control | Requirement | Implementation |
|---------|-------------|----------------|
| **SI-2** | Flaw Remediation | Weekly Trivy scans, 7-day patch SLA |
| **SI-3** | Malicious Code Protection | Falco runtime monitoring |
| **SI-4** | System Monitoring | Prometheus, Falco, structured logs |
| **AC-2** | Account Management | Non-root user, no password auth |
| **SC-7** | Boundary Protection | Network policies, ingress TLS |

### NIS2 Directive (EU)

| Article | Requirement | Implementation |
|---------|-------------|----------------|
| **Article 21** | Risk Management | Documented risk assessment, .trivyignore |
| **Article 23** | Incident Reporting | Automated alerts, 24h/72h/1-month capability |
| **Article 24** | Vulnerability Database | Weekly CVE monitoring, GitHub issues |

### ISO 27001:2022

| Control | Requirement | Implementation |
|---------|-------------|----------------|
| **A.8.1** | User Endpoint Devices | Hardened containers, minimal attack surface |
| **A.8.9** | Configuration Management | Immutable infrastructure, GitOps |
| **A.8.12** | Data Leakage Prevention | Read-only filesystem, network policies |

### FedRAMP Moderate

- ✅ Continuous monitoring (weekly Trivy scans)
- ✅ SBOM generation (syft)
- ✅ Vulnerability tracking (GitHub issues)
- ✅ Incident response (24-hour SLA for CRITICAL)
- ✅ Encryption at rest and in transit

### Evidence Collection

```bash
# Generate compliance report
trivy image fraiseql-server:local \
  --format template \
  --template "@contrib/html.tpl" \
  --output fraiseql-compliance-report.html

# Generate SBOM for audit
syft fraiseql-server:local \
  -o spdx-json \
  --file fraiseql-sbom.spdx.json

# Export security policies
kubectl get networkpolicies,podsecuritypolicies,securitycontextconstraints \
  -n fraiseql-production \
  -o yaml > security-policies-export.yaml
```

---

## Troubleshooting

### Issue 1: Container Won't Start (Read-Only Filesystem)

**Symptoms**: Container crashes with "Read-only file system" error

**Solution**: Ensure `/tmp` and cache directories are writable:

```yaml
# Kubernetes
volumeMounts:
- name: tmp
  mountPath: /tmp
- name: cache
  mountPath: /var/cache

volumes:
- name: tmp
  emptyDir: {}
- name: cache
  emptyDir: {}
```

```bash
# Docker
docker run --read-only \
  --tmpfs /tmp:rw,noexec,nosuid,size=100m \
  --tmpfs /var/cache:rw,noexec,nosuid,size=50m \
  fraiseql-server:local
```

### Issue 2: Permission Denied Errors

**Symptoms**: "Permission denied" when accessing files/directories

**Solution**: Check user ownership and permissions:

```bash
# Debug: Run with shell access (development only)
docker run --rm -it \
  --entrypoint /bin/bash \
  fraiseql-server:local

# Check user ID
id
# Expected: uid=65532(fraiseql) gid=65532(fraiseql)

# Check file ownership
ls -la /app
# Expected: fraiseql:fraiseql ownership
```

### Issue 3: Network Policy Blocking Connections

**Symptoms**: Database connection timeouts, DNS resolution failures

**Solution**: Verify network policies allow required traffic:

```bash
# List network policies
kubectl get networkpolicies -n fraiseql-production

# Test database connectivity
kubectl run -n fraiseql-production test-pod \
  --image=postgres:16-alpine \
  --rm -it --restart=Never \
  -- psql "postgresql://user:pass@postgres:5432/db"

# Check DNS resolution
kubectl run -n fraiseql-production test-pod \
  --image=busybox \
  --rm -it --restart=Never \
  -- nslookup postgres
```

### Issue 4: High/Critical Vulnerabilities Detected

**Symptoms**: Trivy scan shows new HIGH/CRITICAL vulnerabilities

**Response Plan**:

1. **Assess Impact** (< 4 hours)

   ```bash
   # Get vulnerability details
   trivy image fraiseql-server:local \
     --severity HIGH,CRITICAL \
     --format json | jq
   ```

2. **Check Exploitability**
   - Review CVE details in NVD
   - Assess if vulnerability is exploitable in FraiseQL context
   - Document findings

3. **Apply Patch** (< 7 days for HIGH, < 24 hours for CRITICAL)

   ```bash
   # Pull the latest base images (builder and runtime)
   docker pull rust:1.94.1-slim
   docker pull debian:bookworm-slim

   # Rebuild
   docker build -t fraiseql-server:local .

   # Re-scan
   trivy image fraiseql-server:local --severity HIGH,CRITICAL
   ```

4. **Deploy Update**

   ```bash
   # Canary deployment
   kubectl set image deployment/fraiseql \
     fraiseql=ghcr.io/fraiseql/server:2.15.0 \
     -n fraiseql-production

   # Monitor rollout
   kubectl rollout status deployment/fraiseql -n fraiseql-production
   ```

5. **Verify Fix**

   ```bash
   # Final scan
   trivy image fraiseql-server:local

   # Update .trivyignore if needed
   # Remove fixed CVEs
   ```

---

## Rate Limiter Degradation Mode

When Redis is unavailable, the rate limiter operates in **fail-open** mode:
all requests are allowed through, and a cumulative error counter is incremented.

**Why fail-open**: A fail-closed rate limiter would cause a total service outage
whenever Redis is unavailable, which is worse than temporarily allowing unthrottled
traffic. The trade-off is explicitly chosen.

**Monitoring**: Watch the `fraiseql_rate_limit_redis_errors_total` Prometheus metric.
Alert when this counter increases, indicating Redis connectivity issues.

**Mitigation**: Deploy Redis with high availability (Sentinel or Cluster) to minimize
fail-open windows. Consider adding a local in-memory fallback rate limiter for
critical endpoints.

---

## Audit Log Integrity

Audit entries form a SHA256 hash chain: each entry includes the hash of the
previous entry, providing tamper detection within the PostgreSQL table.

**Limitation**: The integrity chain is stored in the same database as the audit
data. An attacker with direct database write access could recompute the entire
chain after modification.

**Recommendation for high-compliance environments** (SOC2, PCI-DSS):

1. Stream audit logs to an external immutable store (S3 with Object Lock, CloudWatch
   Logs, or a SIEM like Splunk/Loki)
2. Use the `fraiseql_audit_logs` table as the primary source, external store as
   tamper-evident backup
3. Periodically verify chain integrity: `SELECT integrity_hash FROM fraiseql_audit_logs ORDER BY id`

### External Audit Log Export

FraiseQL supports streaming audit entries to external systems via pluggable
export sinks, configured in `fraiseql.toml`:

```toml
[fraiseql.security.audit_logging]
enabled = true
log_level = "info"

# Optional: stream audit entries to syslog (requires audit-syslog feature)
# [fraiseql.security.audit_logging.export.syslog]
# address = "syslog.internal"
# port = 514
# protocol = "tcp"   # "tcp" or "udp"

# Optional: stream audit entries to a webhook (requires audit-webhook feature)
# [fraiseql.security.audit_logging.export.webhook]
# url = "https://logs.example.com/ingest"
# batch_size = 100
# flush_interval_secs = 30
# headers = { "Authorization" = "Bearer <token>" }
```

**Integration patterns for common external stores:**

| Store | Method | Notes |
|-------|--------|-------|
| **S3 (immutable)** | Fluent Bit sidecar consuming syslog | Enable S3 Object Lock for WORM compliance |
| **Splunk** | Webhook export → Splunk HEC endpoint | Set `url` to `https://splunk:8088/services/collector/event` |
| **Grafana Loki** | Syslog export → Loki syslog receiver | Use TCP transport for reliable delivery |
| **CloudWatch Logs** | Fluent Bit sidecar with CloudWatch output | Works with both syslog and webhook |
| **Elasticsearch** | Webhook export → Elasticsearch bulk API | Use `_bulk` endpoint with NDJSON |

**Recommendation for SOC2/PCI compliance:** Use the webhook exporter to push
audit entries to an immutable store (S3 with Object Lock, or a SIEM with
write-once retention). The PostgreSQL audit table remains the primary source;
the external store provides a tamper-evident backup that survives database
compromise.

---

## OpenTelemetry Distributed Tracing

FraiseQL ships with OpenTelemetry support compiled in by default. OTLP export
activates only when an endpoint is configured — there is zero overhead otherwise.

### Configuration

Set the OTLP endpoint via environment variable or config file:

```bash
# Environment variable (takes effect without config file changes)
OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317

# Or in fraiseql.toml / ServerConfig
otlp_endpoint = "http://otel-collector:4317"
otlp_export_timeout_secs = 10
tracing_service_name = "fraiseql"
```

### Kubernetes with OpenTelemetry Collector

Deploy an OpenTelemetry Collector as a sidecar or DaemonSet, then point
FraiseQL at it:

```yaml
env:
  - name: OTEL_EXPORTER_OTLP_ENDPOINT
    value: "http://otel-collector:4317"
```

### Security Considerations

- OTLP traffic should stay within the cluster network (use ClusterIP services).
- If exporting to an external collector, use TLS (`https://`) endpoints.
- Trace data may contain query text and field names — ensure your collector
  and backend have appropriate access controls.

---

## Additional Resources

- **Security Remediation Plan**: `docs/security/vulnerability-remediation-plan.md`
- **Distroless Assessment**: `security-assessment-2025-12-09-distroless.md`
- **Weekly Security Alerts**: `.github/workflows/security-alerts.yml`
- **Trivy Exceptions**: `.trivyignore`
- **Falco Rules**: `deploy/security/falco-rules.yaml`

## Support

For security issues, contact: security@fraiseql.io
For general questions: docs@fraiseql.io

---

**Last Updated**: 2025-12-09
**Document Version**: 1.0
**Approved By**: Security Team
