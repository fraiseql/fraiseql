# Kubernetes Deployment for FraiseQL

Enterprise-ready Kubernetes deployment manifests and Helm chart for FraiseQL GraphQL framework.

## 🚀 Quick Start

### Option 1: Using Helm (Recommended)

```bash
# Install with default values
helm install fraiseql ./helm/fraiseql

# Install with custom values
helm install fraiseql ./helm/fraiseql -f values-production.yaml

# Upgrade
helm upgrade fraiseql ./helm/fraiseql
```

### Option 2: Using kubectl

```bash
# Create namespace
kubectl create namespace fraiseql

# Apply secrets
kubectl apply -f secrets.yaml

# Apply config
kubectl apply -f configmap.yaml

# Apply deployment
kubectl apply -f deployment.yaml
kubectl apply -f service.yaml
kubectl apply -f ingress.yaml
kubectl apply -f hpa.yaml
```

## 📁 Directory Structure

```
kubernetes/
├── deployment.yaml          # Main deployment with health checks
├── service.yaml            # ClusterIP and headless services
├── configmap.yaml          # Application configuration
├── secrets.yaml.example    # Secrets template (DO NOT commit actual secrets!)
├── ingress.yaml           # Ingress with TLS
├── hpa.yaml              # Horizontal Pod Autoscaler + PDB
├── helm/                 # Helm chart
│   └── fraiseql/
│       ├── Chart.yaml
│       ├── values.yaml
│       ├── templates/
│       └── README.md
└── README.md            # This file
```

## 🏥 Health Checks

FraiseQL provides **composable health check utilities** that applications use to implement health endpoints:

### How It Works

1. **Framework provides utilities** (`fraiseql.monitoring`)
2. **Application implements endpoints** using those utilities
3. **Kubernetes probes** call those endpoints

### Example Application Code

```python
from fraiseql.monitoring import HealthCheck
from fraiseql.monitoring.health_checks import check_database, check_pool_stats

# Create health check instance
health = HealthCheck()
health.add_check("database", check_database)
health.add_check("pool", check_pool_stats)

# Liveness probe - simple check
@app.get("/health")
async def liveness():
    return {"status": "healthy"}

# Readiness probe - full checks
@app.get("/ready")
async def readiness():
    result = await health.run_checks()
    status_code = 200 if result["status"] == "healthy" else 503
    return Response(content=json.dumps(result), status_code=status_code)
```

### Kubernetes Configuration

```yaml
# Liveness probe - is the pod alive?
livenessProbe:
  httpGet:
    path: /health
    port: 8000
  initialDelaySeconds: 10
  periodSeconds: 30

# Readiness probe - can it serve traffic?
readinessProbe:
  httpGet:
    path: /ready
    port: 8000
  initialDelaySeconds: 5
  periodSeconds: 10

# Startup probe - slow startup support
startupProbe:
  httpGet:
    path: /health
    port: 8000
  failureThreshold: 30  # 150 seconds max
  periodSeconds: 5
```

## 🔐 Secrets Management

### Create Database Credentials

```bash
kubectl create secret generic fraiseql-secrets \
  --from-literal=DB_USER=fraiseql \
  --from-literal=DB_PASSWORD=$(openssl rand -base64 24) \
  --from-literal=JWT_SECRET=$(openssl rand -base64 32) \
  --from-literal=CSRF_SECRET=$(openssl rand -base64 32) \
  --from-literal=SENTRY_DSN=https://your-sentry-dsn
```

### Using External Secrets Operator

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: fraiseql-secrets
spec:
  secretStoreRef:
    name: aws-secrets-manager
  target:
    name: fraiseql-secrets
  data:
    - secretKey: DB_PASSWORD
      remoteRef:
        key: fraiseql/database
        property: password
```

## ⚙️ Configuration

### Key Configuration Options

```yaml
# configmap.yaml
data:
  # Performance
  JSON_PASSTHROUGH_ENABLED: "true"    # 99% faster responses
  TURBO_ROUTER_ENABLED: "true"        # Pre-compiled queries
  APQ_ENABLED: "true"                  # Automatic Persisted Queries

  # Security
  GRAPHQL_DEPTH_LIMIT: "10"
  GRAPHQL_COMPLEXITY_LIMIT: "1000"
  RATE_LIMIT_REQUESTS: "100"

  # Database
  DB_POOL_MIN_SIZE: "5"
  DB_POOL_MAX_SIZE: "20"
```

## 📊 Monitoring

### Prometheus Metrics

```yaml
# Scrape configuration
podAnnotations:
  prometheus.io/scrape: "true"
  prometheus.io/port: "8000"
  prometheus.io/path: "/metrics"
```

Metrics exposed:
- `graphql_requests_total` - Total GraphQL requests
- `graphql_request_duration_seconds` - Request latency histogram
- `database_connections_total` - DB connection pool stats
- `cache_hit_rate` - Cache effectiveness
- `apq_hit_rate` - APQ cache hit rate

### OpenTelemetry Tracing

```yaml
env:
  - name: TRACING_ENABLED
    value: "true"
  - name: OTEL_EXPORTER_OTLP_ENDPOINT
    value: "http://jaeger-collector:4317"
  - name: TRACING_SAMPLE_RATE
    value: "0.1"  # 10% sampling
```

## 📈 Scaling

### Horizontal Pod Autoscaler

```yaml
# hpa.yaml
spec:
  minReplicas: 3
  maxReplicas: 20
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        averageUtilization: 70
  - type: Pods
    pods:
      metric:
        name: graphql_requests_per_second
      target:
        averageValue: "100"
```

### Pod Disruption Budget

```yaml
spec:
  minAvailable: 2  # Always keep 2 pods running during updates
```

## 🌐 Ingress

### NGINX Ingress

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  annotations:
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    nginx.ingress.kubernetes.io/limit-rps: "100"
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
spec:
  tls:
    - secretName: fraiseql-tls
      hosts:
        - api.yourdomain.com
```

### AWS Application Load Balancer

```yaml
metadata:
  annotations:
    kubernetes.io/ingress.class: alb
    alb.ingress.kubernetes.io/scheme: internet-facing
    alb.ingress.kubernetes.io/target-type: ip
    alb.ingress.kubernetes.io/healthcheck-path: /health
```

### GCP Load Balancer

```yaml
metadata:
  annotations:
    kubernetes.io/ingress.class: gce
    kubernetes.io/ingress.global-static-ip-name: "fraiseql-ip"
```

## 🔒 Security

### Pod Security Context

```yaml
securityContext:
  runAsNonRoot: true
  runAsUser: 1000
  fsGroup: 1000
  capabilities:
    drop:
      - ALL
```

### Network Policy

```yaml
# Restrict ingress to nginx-ingress only
spec:
  policyTypes:
    - Ingress
  ingress:
    - from:
      - podSelector:
          matchLabels:
            app: nginx-ingress
```

## 🚀 Deployment Workflow

### 1. Development

```bash
# Deploy to dev namespace
helm install fraiseql-dev ./helm/fraiseql \
  -f values-dev.yaml \
  --namespace dev
```

### 2. Staging

```bash
# Deploy to staging with reduced replicas
helm install fraiseql-staging ./helm/fraiseql \
  -f values-staging.yaml \
  --namespace staging \
  --set replicaCount=2
```

### 3. Production

```bash
# Deploy to production with all features
helm install fraiseql-prod ./helm/fraiseql \
  -f values-production.yaml \
  --namespace production

# Verify deployment
kubectl rollout status deployment/fraiseql-prod -n production
```

### 4. Rolling Update

```bash
# Update image version
helm upgrade fraiseql-prod ./helm/fraiseql \
  --set image.tag=0.11.0 \
  --reuse-values
```

## 🛠️ Troubleshooting

### Check Pod Status

```bash
kubectl get pods -l app=fraiseql
kubectl describe pod <pod-name>
kubectl logs <pod-name> --tail=100 -f
```

### Check Health Endpoints

```bash
# Port forward
kubectl port-forward svc/fraiseql 8000:80

# Test health
curl http://localhost:8000/health
curl http://localhost:8000/ready

# Check metrics
curl http://localhost:8000/metrics
```

### Debug Connection Issues

```bash
# Test database connection from pod
kubectl exec -it <pod-name> -- sh
wget -O- http://localhost:8000/ready

# Check environment variables
kubectl exec <pod-name> -- env | grep DB_
```

### Check HPA Status

```bash
kubectl get hpa fraiseql
kubectl describe hpa fraiseql
```

## 📊 Production Checklist

Before deploying to production:

- [ ] Database credentials in Kubernetes secrets
- [ ] TLS certificates configured (Let's Encrypt or custom)
- [ ] Sentry DSN configured for error tracking
- [ ] Resource limits set appropriately
- [ ] HPA configured for expected traffic
- [ ] PodDisruptionBudget ensures availability
- [ ] Monitoring/alerting configured (Prometheus, Grafana)
- [ ] Network policies restrict traffic
- [ ] Backup strategy for database
- [ ] Log aggregation configured (ELK, Loki, CloudWatch)

## 🏢 Enterprise Features

### Multi-Region Deployment

```yaml
# Use topology spread constraints
topologySpreadConstraints:
  - maxSkew: 1
    topologyKey: topology.kubernetes.io/zone
    whenUnsatisfiable: DoNotSchedule
```

### Priority Classes

```yaml
apiVersion: scheduling.k8s.io/v1
kind: PriorityClass
metadata:
  name: fraiseql-critical
value: 1000000
globalDefault: false
description: "Critical FraiseQL workloads"
```

## 📚 Additional Resources

- [Helm Chart Documentation](./helm/fraiseql/README.md)
- [FraiseQL Documentation](https://fraiseql.com/docs)
- [Kubernetes Best Practices](https://kubernetes.io/docs/concepts/configuration/overview/)
- [Production Readiness Checklist](https://kubernetes.io/docs/tasks/run-application/run-replicated-stateful-application/)

## 💬 Support

- GitHub Issues: https://github.com/fraiseql/fraiseql/issues
- Enterprise Support: contact@fraiseql.com
- Community: Discord/Slack (TBD)
