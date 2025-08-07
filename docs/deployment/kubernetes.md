# Kubernetes Deployment Guide

## Overview

Deploy FraiseQL on Kubernetes for enterprise-grade scalability, high availability, and automated operations. This guide covers everything from basic deployments to advanced Helm charts.

## Prerequisites

- Kubernetes 1.24+
- kubectl configured
- Helm 3.0+ (optional but recommended)
- 4GB+ RAM available in cluster
- PostgreSQL operator or external database

## Quick Start

```bash
# Create namespace
kubectl create namespace fraiseql

# Apply basic deployment
kubectl apply -f fraiseql-deployment.yaml -n fraiseql

# Check status
kubectl get pods -n fraiseql
kubectl get svc -n fraiseql
```

## Kubernetes Manifests

### ConfigMap

```yaml
# configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fraiseql-config
  namespace: fraiseql
data:
  FRAISEQL_MODE: "production"
  LOG_LEVEL: "INFO"
  MAX_CONNECTIONS: "100"
  STATEMENT_TIMEOUT: "30000"
  QUERY_COMPLEXITY_LIMIT: "1000"
  CORS_ORIGINS: "https://app.example.com"
  PROMETHEUS_ENABLED: "true"
  METRICS_PORT: "9090"
```

### Secret

```yaml
# secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: fraiseql-secrets
  namespace: fraiseql
type: Opaque
stringData:
  DATABASE_URL: "postgresql://user:password@postgres:5432/fraiseql"
  SECRET_KEY: "your-secret-key-here"
  JWT_SECRET: "your-jwt-secret"
  REDIS_URL: "redis://:password@redis:6379/0"
```

### Deployment

```yaml
# deployment.yaml
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
        prometheus.io/port: "9090"
        prometheus.io/path: "/metrics"
    spec:
      serviceAccountName: fraiseql
      securityContext:
        runAsNonRoot: true
        runAsUser: 1001
        fsGroup: 1001
      containers:
      - name: fraiseql
        image: fraiseql/api:latest
        imagePullPolicy: Always
        ports:
        - name: http
          containerPort: 8000
          protocol: TCP
        - name: metrics
          containerPort: 9090
          protocol: TCP
        envFrom:
        - configMapRef:
            name: fraiseql-config
        - secretRef:
            name: fraiseql-secrets
        resources:
          requests:
            memory: "512Mi"
            cpu: "250m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
        livenessProbe:
          httpGet:
            path: /health
            port: http
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 5
          successThreshold: 1
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /ready
            port: http
          initialDelaySeconds: 5
          periodSeconds: 5
          timeoutSeconds: 3
          successThreshold: 1
          failureThreshold: 3
        volumeMounts:
        - name: tmp
          mountPath: /tmp
        - name: cache
          mountPath: /app/.cache
      volumes:
      - name: tmp
        emptyDir: {}
      - name: cache
        emptyDir: {}
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
```

### Service

```yaml
# service.yaml
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
  - name: metrics
    port: 9090
    targetPort: metrics
    protocol: TCP
  selector:
    app: fraiseql
  sessionAffinity: ClientIP
  sessionAffinityConfig:
    clientIP:
      timeoutSeconds: 10800
```

### Ingress

```yaml
# ingress.yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: fraiseql
  namespace: fraiseql
  annotations:
    kubernetes.io/ingress.class: nginx
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/rate-limit: "100"
    nginx.ingress.kubernetes.io/proxy-body-size: "10m"
    nginx.ingress.kubernetes.io/proxy-connect-timeout: "60"
    nginx.ingress.kubernetes.io/proxy-send-timeout: "60"
    nginx.ingress.kubernetes.io/proxy-read-timeout: "60"
spec:
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

### Horizontal Pod Autoscaler

```yaml
# hpa.yaml
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
  minReplicas: 2
  maxReplicas: 10
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
  behavior:
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
      - type: Percent
        value: 50
        periodSeconds: 60
    scaleUp:
      stabilizationWindowSeconds: 0
      policies:
      - type: Percent
        value: 100
        periodSeconds: 15
      - type: Pods
        value: 4
        periodSeconds: 15
      selectPolicy: Max
```

### Network Policy

```yaml
# network-policy.yaml
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
    - protocol: TCP
      port: 9090
  egress:
  - to:
    - podSelector:
        matchLabels:
          app: postgres
    ports:
    - protocol: TCP
      port: 5432
  - to:
    - podSelector:
        matchLabels:
          app: redis
    ports:
    - protocol: TCP
      port: 6379
  - to:
    - namespaceSelector: {}
      podSelector:
        matchLabels:
          k8s-app: kube-dns
    ports:
    - protocol: UDP
      port: 53
```

### Pod Disruption Budget

```yaml
# pdb.yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: fraiseql
  namespace: fraiseql
spec:
  minAvailable: 1
  selector:
    matchLabels:
      app: fraiseql
```

## PostgreSQL Deployment

### Using PostgreSQL Operator

```yaml
# postgres-cluster.yaml
apiVersion: postgresql.cnpg.io/v1
kind: Cluster
metadata:
  name: postgres-cluster
  namespace: fraiseql
spec:
  instances: 3
  primaryUpdateStrategy: unsupervised

  postgresql:
    parameters:
      max_connections: "200"
      shared_buffers: "256MB"
      effective_cache_size: "1GB"
      maintenance_work_mem: "128MB"
      checkpoint_completion_target: "0.9"
      wal_buffers: "16MB"
      default_statistics_target: "100"
      random_page_cost: "1.1"
      effective_io_concurrency: "200"

  bootstrap:
    initdb:
      database: fraiseql
      owner: fraiseql
      secret:
        name: postgres-credentials

  storage:
    size: 100Gi
    storageClass: fast-ssd

  monitoring:
    enabled: true

  backup:
    retentionPolicy: "30d"
    barmanObjectStore:
      destinationPath: "s3://backup-bucket/postgres"
      s3Credentials:
        accessKeyId:
          name: backup-credentials
          key: ACCESS_KEY_ID
        secretAccessKey:
          name: backup-credentials
          key: SECRET_ACCESS_KEY
```

### StatefulSet for PostgreSQL

```yaml
# postgres-statefulset.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres
  namespace: fraiseql
spec:
  serviceName: postgres
  replicas: 1
  selector:
    matchLabels:
      app: postgres
  template:
    metadata:
      labels:
        app: postgres
    spec:
      containers:
      - name: postgres
        image: postgres:15-alpine
        ports:
        - containerPort: 5432
          name: postgres
        env:
        - name: POSTGRES_DB
          value: fraiseql
        - name: POSTGRES_USER
          valueFrom:
            secretKeyRef:
              name: postgres-secret
              key: username
        - name: POSTGRES_PASSWORD
          valueFrom:
            secretKeyRef:
              name: postgres-secret
              key: password
        - name: PGDATA
          value: /var/lib/postgresql/data/pgdata
        volumeMounts:
        - name: postgres-storage
          mountPath: /var/lib/postgresql/data
        resources:
          requests:
            memory: "1Gi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "1000m"
  volumeClaimTemplates:
  - metadata:
      name: postgres-storage
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 100Gi
```

## Redis Deployment

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
        command:
        - redis-server
        - --appendonly
        - "yes"
        - --requirepass
        - $(REDIS_PASSWORD)
        ports:
        - containerPort: 6379
        env:
        - name: REDIS_PASSWORD
          valueFrom:
            secretKeyRef:
              name: redis-secret
              key: password
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "200m"
        volumeMounts:
        - name: redis-data
          mountPath: /data
      volumes:
      - name: redis-data
        persistentVolumeClaim:
          claimName: redis-pvc
---
apiVersion: v1
kind: Service
metadata:
  name: redis
  namespace: fraiseql
spec:
  selector:
    app: redis
  ports:
  - port: 6379
    targetPort: 6379
```

## Helm Chart

### Chart Structure

```bash
fraiseql-chart/
├── Chart.yaml
├── values.yaml
├── templates/
│   ├── deployment.yaml
│   ├── service.yaml
│   ├── ingress.yaml
│   ├── configmap.yaml
│   ├── secret.yaml
│   ├── hpa.yaml
│   ├── pdb.yaml
│   └── _helpers.tpl
└── charts/
    └── postgresql/
```

### Chart.yaml

```yaml
apiVersion: v2
name: fraiseql
description: A Helm chart for FraiseQL GraphQL API
type: application
version: 1.0.0
appVersion: "1.0.0"
keywords:
  - fraiseql
  - graphql
  - api
home: https://fraiseql.dev
sources:
  - https://github.com/your-org/fraiseql
maintainers:
  - name: Your Team
    email: team@example.com
dependencies:
  - name: postgresql
    version: "12.x.x"
    repository: https://charts.bitnami.com/bitnami
    condition: postgresql.enabled
  - name: redis
    version: "17.x.x"
    repository: https://charts.bitnami.com/bitnami
    condition: redis.enabled
```

### values.yaml

```yaml
# Default values for fraiseql
replicaCount: 3

image:
  repository: fraiseql/api
  pullPolicy: IfNotPresent
  tag: "latest"

imagePullSecrets: []
nameOverride: ""
fullnameOverride: ""

serviceAccount:
  create: true
  annotations: {}
  name: ""

podAnnotations:
  prometheus.io/scrape: "true"
  prometheus.io/port: "9090"
  prometheus.io/path: "/metrics"

podSecurityContext:
  runAsNonRoot: true
  runAsUser: 1001
  fsGroup: 1001

securityContext:
  capabilities:
    drop:
    - ALL
  readOnlyRootFilesystem: true
  runAsNonRoot: true
  runAsUser: 1001

service:
  type: ClusterIP
  port: 80
  targetPort: 8000
  metricsPort: 9090

ingress:
  enabled: true
  className: "nginx"
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/rate-limit: "100"
  hosts:
    - host: api.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: fraiseql-tls
      hosts:
        - api.example.com

resources:
  limits:
    cpu: 1000m
    memory: 1Gi
  requests:
    cpu: 250m
    memory: 512Mi

autoscaling:
  enabled: true
  minReplicas: 2
  maxReplicas: 10
  targetCPUUtilizationPercentage: 70
  targetMemoryUtilizationPercentage: 80

nodeSelector: {}

tolerations: []

affinity:
  podAntiAffinity:
    preferredDuringSchedulingIgnoredDuringExecution:
    - weight: 100
      podAffinityTerm:
        labelSelector:
          matchExpressions:
          - key: app.kubernetes.io/name
            operator: In
            values:
            - fraiseql
        topologyKey: kubernetes.io/hostname

# Application configuration
config:
  mode: production
  logLevel: INFO
  corsOrigins: "https://app.example.com"
  maxConnections: 100
  statementTimeout: 30000
  queryComplexityLimit: 1000
  prometheusEnabled: true
  metricsPort: 9090

# Secrets - use external secret management in production
secrets:
  databaseUrl: ""
  secretKey: ""
  jwtSecret: ""
  redisUrl: ""

# PostgreSQL subchart configuration
postgresql:
  enabled: true
  auth:
    database: fraiseql
    username: fraiseql
    password: changeme
    postgresPassword: changeme
  primary:
    persistence:
      enabled: true
      size: 100Gi
      storageClass: fast-ssd
    resources:
      requests:
        memory: 1Gi
        cpu: 500m
      limits:
        memory: 2Gi
        cpu: 1000m
    postgresql:
      maxConnections: 200
      sharedBuffers: 256MB

# Redis subchart configuration
redis:
  enabled: true
  auth:
    enabled: true
    password: changeme
  master:
    persistence:
      enabled: true
      size: 10Gi
    resources:
      requests:
        memory: 256Mi
        cpu: 100m
      limits:
        memory: 512Mi
        cpu: 200m

# Monitoring
monitoring:
  enabled: true
  serviceMonitor:
    enabled: true
    interval: 30s
    path: /metrics
```

### Deployment Template

```yaml
# templates/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include "fraiseql.fullname" . }}
  labels:
    {{- include "fraiseql.labels" . | nindent 4 }}
spec:
  {{- if not .Values.autoscaling.enabled }}
  replicas: {{ .Values.replicaCount }}
  {{- end }}
  selector:
    matchLabels:
      {{- include "fraiseql.selectorLabels" . | nindent 6 }}
  template:
    metadata:
      annotations:
        checksum/config: {{ include (print $.Template.BasePath "/configmap.yaml") . | sha256sum }}
        checksum/secret: {{ include (print $.Template.BasePath "/secret.yaml") . | sha256sum }}
        {{- with .Values.podAnnotations }}
        {{- toYaml . | nindent 8 }}
        {{- end }}
      labels:
        {{- include "fraiseql.selectorLabels" . | nindent 8 }}
    spec:
      {{- with .Values.imagePullSecrets }}
      imagePullSecrets:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      serviceAccountName: {{ include "fraiseql.serviceAccountName" . }}
      securityContext:
        {{- toYaml .Values.podSecurityContext | nindent 8 }}
      containers:
      - name: {{ .Chart.Name }}
        securityContext:
          {{- toYaml .Values.securityContext | nindent 12 }}
        image: "{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}"
        imagePullPolicy: {{ .Values.image.pullPolicy }}
        ports:
        - name: http
          containerPort: {{ .Values.service.targetPort }}
          protocol: TCP
        - name: metrics
          containerPort: {{ .Values.config.metricsPort }}
          protocol: TCP
        envFrom:
        - configMapRef:
            name: {{ include "fraiseql.fullname" . }}
        - secretRef:
            name: {{ include "fraiseql.fullname" . }}
        livenessProbe:
          httpGet:
            path: /health
            port: http
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: http
          initialDelaySeconds: 5
          periodSeconds: 5
        resources:
          {{- toYaml .Values.resources | nindent 12 }}
        volumeMounts:
        - name: tmp
          mountPath: /tmp
        - name: cache
          mountPath: /app/.cache
      volumes:
      - name: tmp
        emptyDir: {}
      - name: cache
        emptyDir: {}
      {{- with .Values.nodeSelector }}
      nodeSelector:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.affinity }}
      affinity:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.tolerations }}
      tolerations:
        {{- toYaml . | nindent 8 }}
      {{- end }}
```

## Installation

### Using kubectl

```bash
# Create namespace
kubectl create namespace fraiseql

# Apply all manifests
kubectl apply -f . -n fraiseql

# Wait for rollout
kubectl rollout status deployment/fraiseql -n fraiseql

# Check pods
kubectl get pods -n fraiseql

# View logs
kubectl logs -f deployment/fraiseql -n fraiseql
```

### Using Helm

```bash
# Add Helm repository
helm repo add fraiseql https://charts.fraiseql.dev
helm repo update

# Install with default values
helm install fraiseql fraiseql/fraiseql \
  --namespace fraiseql \
  --create-namespace

# Install with custom values
helm install fraiseql fraiseql/fraiseql \
  --namespace fraiseql \
  --create-namespace \
  --values custom-values.yaml

# Upgrade
helm upgrade fraiseql fraiseql/fraiseql \
  --namespace fraiseql \
  --values custom-values.yaml

# Rollback
helm rollback fraiseql 1 --namespace fraiseql

# Uninstall
helm uninstall fraiseql --namespace fraiseql
```

### Custom Values Example

```yaml
# custom-values.yaml
image:
  tag: "v1.2.3"

ingress:
  enabled: true
  hosts:
    - host: api.mycompany.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: api-tls
      hosts:
        - api.mycompany.com

resources:
  requests:
    memory: "1Gi"
    cpu: "500m"
  limits:
    memory: "2Gi"
    cpu: "2000m"

config:
  corsOrigins: "https://app.mycompany.com"

postgresql:
  auth:
    password: "strong-password-here"
    postgresPassword: "postgres-admin-password"

redis:
  auth:
    password: "redis-password-here"
```

## Monitoring & Observability

### Prometheus ServiceMonitor

```yaml
# servicemonitor.yaml
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
  - port: metrics
    interval: 30s
    path: /metrics
```

### Grafana Dashboard

```json
{
  "dashboard": {
    "title": "FraiseQL Kubernetes Metrics",
    "panels": [
      {
        "title": "Pod CPU Usage",
        "targets": [{
          "expr": "rate(container_cpu_usage_seconds_total{pod=~\"fraiseql-.*\"}[5m])"
        }]
      },
      {
        "title": "Pod Memory Usage",
        "targets": [{
          "expr": "container_memory_usage_bytes{pod=~\"fraiseql-.*\"}"
        }]
      },
      {
        "title": "Request Rate",
        "targets": [{
          "expr": "rate(fraiseql_requests_total[5m])"
        }]
      },
      {
        "title": "Response Time P95",
        "targets": [{
          "expr": "histogram_quantile(0.95, rate(fraiseql_request_duration_seconds_bucket[5m]))"
        }]
      }
    ]
  }
}
```

## Security

### RBAC Configuration

```yaml
# rbac.yaml
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
  resources: ["configmaps", "secrets"]
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

### Pod Security Policy

```yaml
# psp.yaml
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
  readOnlyRootFilesystem: false
```

## Troubleshooting

### Common Issues

#### Pods Not Starting

```bash
# Check pod status
kubectl describe pod <pod-name> -n fraiseql

# Check events
kubectl get events -n fraiseql --sort-by='.lastTimestamp'

# Check logs
kubectl logs <pod-name> -n fraiseql --previous
```

#### Database Connection Issues

```bash
# Test database connection
kubectl run -it --rm debug --image=postgres:15 --restart=Never -n fraiseql -- \
  psql postgresql://user:pass@postgres:5432/fraiseql -c "SELECT 1"

# Check DNS resolution
kubectl run -it --rm debug --image=busybox --restart=Never -n fraiseql -- \
  nslookup postgres
```

#### Image Pull Errors

```bash
# Check image pull secrets
kubectl get secrets -n fraiseql

# Create image pull secret
kubectl create secret docker-registry regcred \
  --docker-server=<registry> \
  --docker-username=<username> \
  --docker-password=<password> \
  --docker-email=<email> \
  -n fraiseql
```

### Debug Pod

```yaml
# debug-pod.yaml
apiVersion: v1
kind: Pod
metadata:
  name: debug
  namespace: fraiseql
spec:
  containers:
  - name: debug
    image: fraiseql/api:latest
    command: ["/bin/bash"]
    args: ["-c", "sleep 3600"]
    envFrom:
    - configMapRef:
        name: fraiseql-config
    - secretRef:
        name: fraiseql-secrets
```

## Best Practices

1. **Use namespaces** to isolate environments
2. **Set resource limits** to prevent resource exhaustion
3. **Use HPA** for automatic scaling
4. **Implement PDB** for high availability
5. **Use secrets management** (Sealed Secrets, External Secrets)
6. **Enable network policies** for security
7. **Set up monitoring** from day one
8. **Use GitOps** for deployment (ArgoCD, Flux)
9. **Regular backups** of persistent data
10. **Test disaster recovery** procedures

## Next Steps

1. Set up [monitoring](./monitoring.md) with Prometheus and Grafana
2. Configure [auto-scaling](./scaling.md) strategies
3. Deploy to [cloud platforms](./aws.md)
4. Review [production checklist](./production-checklist.md)
