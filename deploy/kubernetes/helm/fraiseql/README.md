# FraiseQL Helm Chart

High-performance GraphQL framework for PostgreSQL with CQRS, APQ, and sub-millisecond responses.

## Features

- ✅ **Kubernetes-native** deployment with HPA, PDB, health checks
- ✅ **Production-ready** with Sentry, OpenTelemetry, Prometheus metrics
- ✅ **Secure by default** with RBAC, security contexts, network policies
- ✅ **Highly configurable** with 50+ configuration options

## Prerequisites

- Kubernetes 1.21+
- Helm 3.8+
- PostgreSQL 13+ (external or in-cluster)

## Quick Start

```bash
# Add FraiseQL Helm repository (when published)
helm repo add fraiseql https://helm.fraiseql.com
helm repo update

# Install with default values
helm install my-fraiseql fraiseql/fraiseql

# Or install from local chart
helm install my-fraiseql ./deploy/kubernetes/helm/fraiseql
```

## Configuration

### Minimal Production Configuration

```yaml
# values-production.yaml
image:
  tag: "0.11.0"

replicaCount: 3

autoscaling:
  enabled: true
  minReplicas: 3
  maxReplicas: 20

database:
  host: "postgresql.default.svc.cluster.local"
  name: "fraiseql"
  existingSecret: "fraiseql-db-credentials"

ingress:
  enabled: true
  className: "nginx"
  hosts:
    - host: api.yourdomain.com
      paths:
        - path: /graphql
          pathType: Prefix
  tls:
    - secretName: fraiseql-tls
      hosts:
        - api.yourdomain.com

sentry:
  enabled: true
  # DSN should be in existingSecret

secrets:
  existingSecret: "fraiseql-secrets"
```

Install with custom values:
```bash
helm install my-fraiseql fraiseql/fraiseql -f values-production.yaml
```

### Key Configuration Options

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of replicas | `3` |
| `image.repository` | Image repository | `fraiseql/fraiseql` |
| `image.tag` | Image tag | `Chart.appVersion` |
| `autoscaling.enabled` | Enable HPA | `true` |
| `autoscaling.minReplicas` | Min replicas | `3` |
| `autoscaling.maxReplicas` | Max replicas | `20` |
| `database.host` | PostgreSQL host | `postgresql.default.svc.cluster.local` |
| `database.existingSecret` | Secret with DB credentials | `""` |
| `ingress.enabled` | Enable ingress | `true` |
| `sentry.enabled` | Enable Sentry error tracking | `true` |
| `config.apq.enabled` | Enable APQ | `true` |

See [values.yaml](./values.yaml) for all configuration options.

## Secrets Management

### Create Database Secret

```bash
kubectl create secret generic fraiseql-db-credentials \
  --from-literal=DB_USER=fraiseql \
  --from-literal=DB_PASSWORD=your-secure-password
```

### Create Application Secrets

```bash
kubectl create secret generic fraiseql-secrets \
  --from-literal=JWT_SECRET=$(openssl rand -base64 32) \
  --from-literal=CSRF_SECRET=$(openssl rand -base64 32) \
  --from-literal=SENTRY_DSN=https://your-sentry-dsn
```

## Health Checks

FraiseQL uses composable health check utilities:

### Application Implementation

```python
from fraiseql.monitoring import HealthCheck
from fraiseql.monitoring.health_checks import check_database, check_pool_stats

health = HealthCheck()
health.add_check("database", check_database)
health.add_check("pool", check_pool_stats)

@app.get("/health")  # Liveness probe
async def liveness():
    return {"status": "healthy"}

@app.get("/ready")   # Readiness probe
async def readiness():
    result = await health.run_checks()
    status_code = 200 if result["status"] == "healthy" else 503
    return Response(content=json.dumps(result), status_code=status_code)
```

### Kubernetes Configuration

The Helm chart automatically configures:
- **Liveness probe**: `/health` - Simple check, pod is alive
- **Readiness probe**: `/ready` - Full health checks (DB, cache, etc.)
- **Startup probe**: `/health` - Allows slow startup (up to 150s)

## Monitoring

### Prometheus Metrics

Metrics are exposed at `/metrics` on port 8000. Configure Prometheus scraping:

```yaml
podAnnotations:
  prometheus.io/scrape: "true"
  prometheus.io/port: "8000"
  prometheus.io/path: "/metrics"
```

### OpenTelemetry Tracing

Enable distributed tracing:

```yaml
opentelemetry:
  enabled: true
  serviceName: "fraiseql"
  exportEndpoint: "http://jaeger-collector:4317"
  sampleRate: 0.1
```

## Scaling

### Horizontal Pod Autoscaling

```yaml
autoscaling:
  enabled: true
  minReplicas: 3
  maxReplicas: 20
  targetCPUUtilizationPercentage: 70
  targetMemoryUtilizationPercentage: 80
```

### Pod Disruption Budget

Ensures high availability during node maintenance:

```yaml
podDisruptionBudget:
  enabled: true
  minAvailable: 2  # Always keep 2 pods running
```

## Security

### Pod Security

```yaml
podSecurityContext:
  fsGroup: 1000
  runAsNonRoot: true
  runAsUser: 1000

securityContext:
  allowPrivilegeEscalation: false
  capabilities:
    drop:
    - ALL
```

### Network Policy

```yaml
networkPolicy:
  enabled: true
  ingress:
    - from:
      - podSelector:
          matchLabels:
            app: nginx-ingress
  egress:
    - to:
      - podSelector:
          matchLabels:
            app: postgresql
```

## Upgrade

```bash
helm upgrade my-fraiseql fraiseql/fraiseql -f values-production.yaml
```

## Uninstall

```bash
helm uninstall my-fraiseql
```

## Troubleshooting

### Check Pod Status
```bash
kubectl get pods -l app.kubernetes.io/name=fraiseql
kubectl logs -l app.kubernetes.io/name=fraiseql --tail=100
```

### Check Health
```bash
kubectl port-forward svc/my-fraiseql 8000:80
curl http://localhost:8000/health
curl http://localhost:8000/ready
```

### Check Metrics
```bash
curl http://localhost:8000/metrics
```

## Support

- 📚 Documentation: https://fraiseql.com/docs
- 💬 GitHub Issues: https://github.com/fraiseql/fraiseql/issues
- 🏢 Enterprise Support: contact@fraiseql.com
