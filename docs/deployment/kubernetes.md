# Kubernetes Deployment Guide

Deploy FraiseQL on Kubernetes for production-grade container orchestration with automatic scaling, self-healing, and rolling updates.

## Table of Contents
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [Production Setup](#production-setup)
- [Monitoring & Observability](#monitoring--observability)
- [Scaling & Performance](#scaling--performance)
- [Security](#security)
- [Troubleshooting](#troubleshooting)

## Prerequisites

- Kubernetes cluster (1.24+)
- kubectl configured
- Helm 3 (optional but recommended)
- Container registry access

## Quick Start

### 1. Create Namespace

```bash
kubectl create namespace fraiseql
kubectl config set-context --current --namespace=fraiseql
```

### 2. Create Secrets

```bash
# Database credentials
kubectl create secret generic fraiseql-db \
  --from-literal=url='postgresql://user:pass@postgres:5432/fraiseql'

# Auth0 credentials (if using)
kubectl create secret generic fraiseql-auth \
  --from-literal=domain='your-domain.auth0.com' \
  --from-literal=api-identifier='https://api.example.com'
```

### 3. Apply Basic Deployment

```yaml
# Save as fraiseql-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql
  namespace: fraiseql
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql
  template:
    metadata:
      labels:
        app: fraiseql
    spec:
      containers:
      - name: fraiseql
        image: fraiseql:latest
        ports:
        - containerPort: 8000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-db
              key: url
        - name: FRAISEQL_PRODUCTION
          value: "true"
        resources:
          requests:
            cpu: 500m
            memory: 512Mi
          limits:
            cpu: 1000m
            memory: 1Gi
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 5
---
apiVersion: v1
kind: Service
metadata:
  name: fraiseql
  namespace: fraiseql
spec:
  selector:
    app: fraiseql
  ports:
  - port: 80
    targetPort: 8000
  type: LoadBalancer
```

```bash
# Deploy
kubectl apply -f fraiseql-deployment.yaml

# Check status
kubectl get pods
kubectl get svc fraiseql
```

## Configuration

### ConfigMap for Application Settings

```yaml
# fraiseql-config.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fraiseql-config
  namespace: fraiseql
data:
  # Application settings
  APP_NAME: "FraiseQL Production"
  APP_VERSION: "1.0.0"

  # Feature flags
  FRAISEQL_AUTO_CAMEL_CASE: "true"
  FRAISEQL_ENABLE_PLAYGROUND: "false"
  FRAISEQL_ENABLE_INTROSPECTION: "false"

  # Performance
  FRAISEQL_ENABLE_TURBO_ROUTER: "true"
  FRAISEQL_TURBO_ROUTER_CACHE_SIZE: "2000"

  # Monitoring
  FRAISEQL_ENABLE_METRICS: "true"
  FRAISEQL_ENABLE_TRACING: "true"
  FRAISEQL_TRACING_ENDPOINT: "http://jaeger-collector:4317"
  FRAISEQL_TRACING_SAMPLE_RATE: "0.1"
```

### Complete Production Deployment

```yaml
# fraiseql-production.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql
  namespace: fraiseql
  labels:
    app: fraiseql
    version: v1
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
  selector:
    matchLabels:
      app: fraiseql
  template:
    metadata:
      labels:
        app: fraiseql
        version: v1
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8000"
        prometheus.io/path: "/metrics"
    spec:
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
          - weight: 100
            podAffinityTerm:
              labelSelector:
                matchExpressions:
                - key: app
                  operator: In
                  values:
                  - fraiseql
              topologyKey: kubernetes.io/hostname
      containers:
      - name: fraiseql
        image: registry.example.com/fraiseql:v1.0.0
        imagePullPolicy: Always
        ports:
        - name: http
          containerPort: 8000
          protocol: TCP
        - name: metrics
          containerPort: 8000
          protocol: TCP
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-db
              key: url
        - name: AUTH0_DOMAIN
          valueFrom:
            secretKeyRef:
              name: fraiseql-auth
              key: domain
        - name: AUTH0_API_IDENTIFIER
          valueFrom:
            secretKeyRef:
              name: fraiseql-auth
              key: api-identifier
        envFrom:
        - configMapRef:
            name: fraiseql-config
        resources:
          requests:
            cpu: 500m
            memory: 512Mi
          limits:
            cpu: 2000m
            memory: 2Gi
        livenessProbe:
          httpGet:
            path: /health
            port: http
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /ready
            port: http
          initialDelaySeconds: 5
          periodSeconds: 5
          timeoutSeconds: 3
          failureThreshold: 3
        startupProbe:
          httpGet:
            path: /health
            port: http
          initialDelaySeconds: 0
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 30
        volumeMounts:
        - name: tmp
          mountPath: /tmp
        securityContext:
          runAsNonRoot: true
          runAsUser: 1000
          allowPrivilegeEscalation: false
          readOnlyRootFilesystem: true
          capabilities:
            drop:
            - ALL
      volumes:
      - name: tmp
        emptyDir: {}
      imagePullSecrets:
      - name: registry-credentials
---
apiVersion: v1
kind: Service
metadata:
  name: fraiseql
  namespace: fraiseql
  labels:
    app: fraiseql
spec:
  type: ClusterIP
  ports:
  - name: http
    port: 80
    targetPort: http
    protocol: TCP
  selector:
    app: fraiseql
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: fraiseql
  namespace: fraiseql
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
    cert-manager.io/cluster-issuer: letsencrypt-prod
spec:
  ingressClassName: nginx
  tls:
  - hosts:
    - api.example.com
    secretName: fraiseql-tls
  rules:
  - host: api.example.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: fraiseql
            port:
              number: 80
```

## Production Setup

### 1. Horizontal Pod Autoscaler

```yaml
# fraiseql-hpa.yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: fraiseql
  namespace: fraiseql
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql
  minReplicas: 3
  maxReplicas: 20
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
  - type: Pods
    pods:
      metric:
        name: http_requests_per_second
      target:
        type: AverageValue
        averageValue: "100"
  behavior:
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
      - type: Percent
        value: 10
        periodSeconds: 60
    scaleUp:
      stabilizationWindowSeconds: 60
      policies:
      - type: Percent
        value: 50
        periodSeconds: 60
```

### 2. Pod Disruption Budget

```yaml
# fraiseql-pdb.yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: fraiseql
  namespace: fraiseql
spec:
  minAvailable: 2
  selector:
    matchLabels:
      app: fraiseql
```

### 3. Network Policies

```yaml
# fraiseql-network-policy.yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: fraiseql
  namespace: fraiseql
spec:
  podSelector:
    matchLabels:
      app: fraiseql
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: ingress-nginx
    - podSelector:
        matchLabels:
          app: prometheus
    ports:
    - protocol: TCP
      port: 8000
  egress:
  - to:
    - namespaceSelector:
        matchLabels:
          name: database
    ports:
    - protocol: TCP
      port: 5432
  - to:
    - namespaceSelector:
        matchLabels:
          name: monitoring
    ports:
    - protocol: TCP
      port: 4317  # Jaeger
  - ports:
    - protocol: TCP
      port: 53  # DNS
    - protocol: UDP
      port: 53
```

## Monitoring & Observability

### 1. ServiceMonitor for Prometheus

```yaml
# fraiseql-servicemonitor.yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: fraiseql
  namespace: fraiseql
spec:
  selector:
    matchLabels:
      app: fraiseql
  endpoints:
  - port: http
    path: /metrics
    interval: 15s
    scrapeTimeout: 10s
```

### 2. Grafana Dashboard ConfigMap

```yaml
# fraiseql-dashboard.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fraiseql-dashboard
  namespace: monitoring
data:
  fraiseql.json: |
    {
      "dashboard": {
        "title": "FraiseQL Performance",
        "panels": [
          {
            "title": "Request Rate",
            "targets": [{
              "expr": "rate(fraiseql_graphql_queries_total[5m])"
            }]
          },
          {
            "title": "Error Rate",
            "targets": [{
              "expr": "rate(fraiseql_graphql_queries_errors[5m])"
            }]
          },
          {
            "title": "Response Time",
            "targets": [{
              "expr": "histogram_quantile(0.95, fraiseql_response_time_seconds)"
            }]
          }
        ]
      }
    }
```

### 3. Distributed Tracing with Jaeger

```yaml
# jaeger-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: jaeger
  namespace: monitoring
spec:
  replicas: 1
  selector:
    matchLabels:
      app: jaeger
  template:
    metadata:
      labels:
        app: jaeger
    spec:
      containers:
      - name: jaeger
        image: jaegertracing/all-in-one:latest
        ports:
        - containerPort: 16686  # UI
        - containerPort: 4317   # OTLP gRPC
        - containerPort: 4318   # OTLP HTTP
        env:
        - name: COLLECTOR_OTLP_ENABLED
          value: "true"
```

## Scaling & Performance

### Vertical Pod Autoscaler

```yaml
# fraiseql-vpa.yaml
apiVersion: autoscaling.k8s.io/v1
kind: VerticalPodAutoscaler
metadata:
  name: fraiseql
  namespace: fraiseql
spec:
  targetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql
  updatePolicy:
    updateMode: "Auto"
  resourcePolicy:
    containerPolicies:
    - containerName: fraiseql
      minAllowed:
        cpu: 250m
        memory: 256Mi
      maxAllowed:
        cpu: 4
        memory: 4Gi
```

### Cache Layer with Redis

```yaml
# redis-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: redis
  namespace: fraiseql
spec:
  replicas: 1
  selector:
    matchLabels:
      app: redis
  template:
    metadata:
      labels:
        app: redis
    spec:
      containers:
      - name: redis
        image: redis:7-alpine
        ports:
        - containerPort: 6379
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
```

## Security

### 1. RBAC Configuration

```yaml
# fraiseql-rbac.yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: fraiseql
  namespace: fraiseql
---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: fraiseql
  namespace: fraiseql
rules:
- apiGroups: [""]
  resources: ["configmaps"]
  verbs: ["get", "list", "watch"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: fraiseql
  namespace: fraiseql
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: Role
  name: fraiseql
subjects:
- kind: ServiceAccount
  name: fraiseql
  namespace: fraiseql
```

### 2. Pod Security Policy

```yaml
# fraiseql-psp.yaml
apiVersion: policy/v1beta1
kind: PodSecurityPolicy
metadata:
  name: fraiseql
spec:
  privileged: false
  allowPrivilegeEscalation: false
  requiredDropCapabilities:
  - ALL
  volumes:
  - 'configMap'
  - 'emptyDir'
  - 'projected'
  - 'secret'
  - 'downwardAPI'
  - 'persistentVolumeClaim'
  hostNetwork: false
  hostIPC: false
  hostPID: false
  runAsUser:
    rule: 'MustRunAsNonRoot'
  seLinux:
    rule: 'RunAsAny'
  supplementalGroups:
    rule: 'RunAsAny'
  fsGroup:
    rule: 'RunAsAny'
  readOnlyRootFilesystem: true
```

## Troubleshooting

### Common Issues

#### 1. Pods not starting

```bash
# Check pod status
kubectl describe pod <pod-name>

# Check logs
kubectl logs <pod-name> --previous

# Common issues:
# - Image pull errors
# - Missing secrets/configmaps
# - Resource constraints
# - Failing probes
```

#### 2. Database connectivity

```bash
# Test from pod
kubectl exec -it <pod-name> -- bash
apt-get update && apt-get install -y postgresql-client
psql $DATABASE_URL -c "SELECT 1"

# Check DNS resolution
nslookup postgres-service
```

#### 3. Performance issues

```bash
# Check resource usage
kubectl top pods
kubectl top nodes

# Check HPA status
kubectl get hpa fraiseql -w

# Enable debug logging
kubectl set env deployment/fraiseql FRAISEQL_LOG_LEVEL=DEBUG
```

### Debugging Commands

```bash
# Get all resources
kubectl get all -n fraiseql

# Describe deployment
kubectl describe deployment fraiseql

# Check events
kubectl get events -n fraiseql --sort-by='.lastTimestamp'

# Port forward for local debugging
kubectl port-forward svc/fraiseql 8000:80

# Execute commands in pod
kubectl exec -it <pod-name> -- python -c "import fraiseql; print(fraiseql.__version__)"
```

### Monitoring Queries

```promql
# Request rate
sum(rate(fraiseql_graphql_queries_total[5m])) by (operation_type)

# Error rate
sum(rate(fraiseql_graphql_queries_errors[5m])) / sum(rate(fraiseql_graphql_queries_total[5m]))

# P95 latency
histogram_quantile(0.95, sum(rate(fraiseql_response_time_seconds_bucket[5m])) by (le))

# Pod memory usage
sum(container_memory_usage_bytes{pod=~"fraiseql-.*"}) by (pod)
```

## Best Practices

1. **Use namespaces** to isolate environments
2. **Set resource limits** to prevent noisy neighbors
3. **Configure PodDisruptionBudgets** for high availability
4. **Use health checks** appropriately (startup, liveness, readiness)
5. **Enable autoscaling** based on actual metrics
6. **Implement proper security** (RBAC, NetworkPolicies, PSP)
7. **Monitor everything** with Prometheus and Grafana
8. **Use GitOps** for deployment management (ArgoCD, Flux)

## Next Steps

- [Helm Chart](./helm.md) - Package deployments with Helm
- [Cloud Providers](./cloud.md) - EKS, GKE, AKS specific guides
- [GitOps Setup](./gitops.md) - Automated deployments with ArgoCD
- [Disaster Recovery](./disaster-recovery.md) - Backup and restore strategies
