# Deployment & Operations Guide - From Development to Production

**Duration**: 2-4 hours
**Outcome**: Deploy FraiseQL to production with confidence
**Prerequisites**: Completed [GETTING_STARTED.md](../GETTING_STARTED.md)

---

## Overview

This guide covers deploying the FraiseQL GraphQL server to various environments including local development, Docker, Kubernetes, and cloud platforms. See [operations/guide.md](../operations/guide.md) for production operations and maintenance.

## Prerequisites

- Compiled schema file (`schema.compiled.json`)
- Database (PostgreSQL, MySQL, or SQLite)
- For Kubernetes: Docker image and Kubernetes cluster
- For cloud: Appropriate cloud credentials

## Local Development

### Quick Start

```bash
# Set environment variables
export FRAISEQL_SCHEMA_PATH=schema.compiled.json
export DATABASE_URL=postgresql://localhost/fraiseql_dev
export FRAISEQL_POOL_MIN=5
export FRAISEQL_POOL_MAX=10

# Run server
cargo run -p fraiseql-server

# Server starts at http://localhost:8000
```text

### Development Environment Setup

Create `.env.dev`:

```bash
# Server Configuration
FRAISEQL_HOST=127.0.0.1
FRAISEQL_PORT=8000

# Schema
FRAISEQL_SCHEMA_PATH=./schema.compiled.json

# Database
DATABASE_URL=postgresql://devuser:devpass@localhost:5432/fraiseql_dev

# Connection Pool (small for development)
FRAISEQL_POOL_MIN=2
FRAISEQL_POOL_MAX=5
FRAISEQL_POOL_TIMEOUT_SECS=10

# Query Validation (permissive for development)
FRAISEQL_MAX_QUERY_DEPTH=20
FRAISEQL_MAX_QUERY_COMPLEXITY=500

# Logging
RUST_LOG=debug
```text

Load environment:

```bash
source .env.dev
cargo run -p fraiseql-server
```text

### Local Database Setup

PostgreSQL (with Docker):

```bash
docker run --name fraiseql-dev \
  -e POSTGRES_DB=fraiseql_dev \
  -e POSTGRES_USER=devuser \
  -e POSTGRES_PASSWORD=devpass \
  -p 5432:5432 \
  -d postgres:15

# Wait for startup
sleep 5

# Connect and verify
psql -h localhost -U devuser -d fraiseql_dev -c "SELECT 1"
```text

SQLite (simplest for testing):

```bash
# Create in-memory database for testing
DATABASE_URL=sqlite::memory: cargo run -p fraiseql-server
```text

## Docker Deployment

### Build Docker Image

Create `Dockerfile`:

```dockerfile
# Builder stage
FROM rust:1.75 as builder

WORKDIR /app
COPY . .

# Build release binary
RUN cargo build --release -p fraiseql-server

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/fraiseql-server /usr/local/bin/

# Create app directory
WORKDIR /app

# Copy schema file
COPY schema.compiled.json .

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Set default environment
ENV FRAISEQL_SCHEMA_PATH=/app/schema.compiled.json

# Run server
CMD ["fraiseql-server"]
```text

Build image:

```bash
docker build -t fraiseql-server:v2.0 .
```text

### Run Docker Container

```bash
docker run -d \
  --name fraiseql \
  -p 8000:8000 \
  -e DATABASE_URL=postgresql://user:pass@db:5432/fraiseql \
  -e FRAISEQL_POOL_MIN=10 \
  -e FRAISEQL_POOL_MAX=50 \
  fraiseql-server:v2.0
```text

### Docker Compose (Development)

Create `docker-compose.yml`:

```yaml
version: '3.8'

services:
  postgres:
    image: postgres:15
    container_name: fraiseql-db
    environment:
      POSTGRES_DB: fraiseql_dev
      POSTGRES_USER: devuser
      POSTGRES_PASSWORD: devpass
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U devuser"]
      interval: 10s
      timeout: 5s
      retries: 5

  fraiseql:
    build: .
    container_name: fraiseql-server
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      FRAISEQL_HOST: 0.0.0.0
      FRAISEQL_PORT: 8000
      FRAISEQL_SCHEMA_PATH: /app/schema.compiled.json
      DATABASE_URL: postgresql://devuser:devpass@postgres:5432/fraiseql_dev
      FRAISEQL_POOL_MIN: 5
      FRAISEQL_POOL_MAX: 20
      FRAISEQL_MAX_QUERY_DEPTH: 10
      FRAISEQL_MAX_QUERY_COMPLEXITY: 100
      RUST_LOG: info
    ports:
      - "8000:8000"
    volumes:
      - ./schema.compiled.json:/app/schema.compiled.json

volumes:
  postgres_data:
```text

Start services:

```bash
docker-compose up -d

# View logs
docker-compose logs -f fraiseql

# Stop services
docker-compose down
```text

## Kubernetes Deployment

### Prerequisites

- Kubernetes cluster (1.20+)
- kubectl configured
- Database running outside cluster or in separate StatefulSet
- Docker image pushed to registry

### ConfigMap (Configuration)

Create `k8s/configmap.yaml`:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fraiseql-config
  namespace: default
data:
  FRAISEQL_HOST: "0.0.0.0"
  FRAISEQL_PORT: "8000"
  FRAISEQL_SCHEMA_PATH: "/app/schema.compiled.json"
  FRAISEQL_POOL_MIN: "10"
  FRAISEQL_POOL_MAX: "50"
  FRAISEQL_POOL_TIMEOUT_SECS: "30"
  FRAISEQL_MAX_QUERY_DEPTH: "10"
  FRAISEQL_MAX_QUERY_COMPLEXITY: "100"
```text

### Secret (Database Credentials)

Create `k8s/secret.yaml`:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: fraiseql-db-secret
  namespace: default
type: Opaque
stringData:
  DATABASE_URL: postgresql://user:password@db.prod.internal:5432/fraiseql_prod
```text

### Deployment

Create `k8s/deployment.yaml`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-server
  namespace: default
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
  selector:
    matchLabels:
      app: fraiseql-server
  template:
    metadata:
      labels:
        app: fraiseql-server
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8000"
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
                  - fraiseql-server
              topologyKey: kubernetes.io/hostname

      containers:
      - name: fraiseql-server
        image: myregistry/fraiseql-server:v2.0
        imagePullPolicy: Always

        ports:
        - name: http
          containerPort: 8000
          protocol: TCP

        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-db-secret
              key: DATABASE_URL

        envFrom:
        - configMapRef:
            name: fraiseql-config

        resources:
          requests:
            cpu: 250m
            memory: 256Mi
          limits:
            cpu: 500m
            memory: 512Mi

        livenessProbe:
          httpGet:
            path: /health
            port: http
          initialDelaySeconds: 10
          periodSeconds: 10
          timeoutSeconds: 3
          failureThreshold: 3

        readinessProbe:
          httpGet:
            path: /health
            port: http
          initialDelaySeconds: 5
          periodSeconds: 5
          timeoutSeconds: 2
          failureThreshold: 2

        lifecycle:
          preStop:
            exec:
              command: ["/bin/sh", "-c", "sleep 15"]
```text

### Service

Create `k8s/service.yaml`:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: fraiseql-server
  namespace: default
spec:
  type: LoadBalancer
  selector:
    app: fraiseql-server
  ports:
  - name: http
    port: 80
    targetPort: 8000
    protocol: TCP
```text

### Deploy to Kubernetes

```bash
# Create namespace
kubectl create namespace fraiseql

# Apply configuration
kubectl apply -f k8s/configmap.yaml
kubectl apply -f k8s/secret.yaml
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/service.yaml

# Verify deployment
kubectl get deployment fraiseql-server
kubectl get pods -l app=fraiseql-server

# View logs
kubectl logs -f deployment/fraiseql-server

# Port forward for testing
kubectl port-forward service/fraiseql-server 8000:80

# Test
curl http://localhost:8000/health
```text

## AWS Deployment

### ECS (Elastic Container Service)

1. **Create ECR Repository**:

```bash
aws ecr create-repository --repository-name fraiseql-server
```text

1. **Push Image**:

```bash
docker tag fraiseql-server:v2.0 {account}.dkr.ecr.us-east-1.amazonaws.com/fraiseql-server:v2.0
aws ecr get-login-password | docker login --username AWS --password-stdin {account}.dkr.ecr.us-east-1.amazonaws.com
docker push {account}.dkr.ecr.us-east-1.amazonaws.com/fraiseql-server:v2.0
```text

1. **Create RDS Database**:

```bash
aws rds create-db-instance \
  --db-instance-identifier fraiseql-prod \
  --db-instance-class db.t3.micro \
  --engine postgres \
  --master-username admin \
  --allocated-storage 20
```text

1. **Create ECS Task Definition** (in AWS Console):

- Container image: ECR URL
- Memory: 512 MB
- CPU: 256 units
- Environment variables: DATABASE_URL, FRAISEQL_* configs
- Port mappings: 8000:8000

1. **Create ECS Service**:

```bash
aws ecs create-service \
  --cluster fraiseql-prod \
  --service-name fraiseql-server \
  --task-definition fraiseql-server:1 \
  --desired-count 3
```text

### Lambda + API Gateway (Serverless)

Not recommended for FraiseQL due to connection pooling and persistent connection requirements.

## Google Cloud Deployment

### Cloud Run

1. **Build and Push**:

```bash
gcloud builds submit --tag gcr.io/PROJECT_ID/fraiseql-server

# Or manually
docker tag fraiseql-server:v2.0 gcr.io/PROJECT_ID/fraiseql-server:v2.0
docker push gcr.io/PROJECT_ID/fraiseql-server:v2.0
```text

1. **Deploy**:

```bash
gcloud run deploy fraiseql-server \
  --image gcr.io/PROJECT_ID/fraiseql-server:v2.0 \
  --platform managed \
  --region us-central1 \
  --allow-unauthenticated \
  --set-env-vars=DATABASE_URL=postgresql://... \
  --memory 512Mi \
  --cpu 1
```text

### GKE (Google Kubernetes Engine)

Follow Kubernetes section above, then:

```bash
gcloud container clusters create fraiseql-cluster \
  --num-nodes 3 \
  --machine-type n1-standard-2

kubectl apply -f k8s/
```text

## Azure Deployment

### Container Instances

```bash
az container create \
  --resource-group fraiseql-rg \
  --name fraiseql-server \
  --image myregistry.azurecr.io/fraiseql-server:v2.0 \
  --cpu 1 \
  --memory 1 \
  --environment-variables \
    DATABASE_URL=postgresql://... \
  --ports 8000
```text

### App Service

```bash
az appservice plan create \
  --name fraiseql-plan \
  --resource-group fraiseql-rg \
  --sku B2 --is-linux

az webapp create \
  --resource-group fraiseql-rg \
  --plan fraiseql-plan \
  --name fraiseql-server \
  --deployment-container-image-name myregistry/fraiseql-server:v2.0
```text

## Production Checklist

- [ ] Database backups configured
- [ ] Connection pool limits tuned for load
- [ ] Query validation limits appropriate for use case
- [ ] Health checks configured and tested
- [ ] Monitoring and alerting set up
- [ ] Log aggregation configured
- [ ] SSL/TLS certificates installed
- [ ] CORS headers configured properly
- [ ] Database credentials in secrets management (not hardcoded)
- [ ] Container image security scanning enabled
- [ ] Resource limits set (CPU, memory)
- [ ] Graceful shutdown configured (preStop hook)
- [ ] Load testing performed
- [ ] Disaster recovery plan documented

## Monitoring

### Key Metrics

```bash
# Check health endpoint
curl http://localhost:8000/health

# Monitor connection pool
# Connection pool metrics shown in health response

# Database connection status
SELECT count(*) FROM pg_stat_activity WHERE datname = 'fraiseql_prod';
```text

### Prometheus Metrics (Future)

```text
fraiseql_query_duration_seconds
fraiseql_query_errors_total
fraiseql_connection_pool_active
fraiseql_connection_pool_idle
```text

## Troubleshooting

### Server Won't Start

Check logs:

```bash
# Docker
docker logs fraiseql

# Kubernetes
kubectl logs deployment/fraiseql-server

# Local
RUST_LOG=debug cargo run -p fraiseql-server
```text

Common issues:

- Schema file not found: Check `FRAISEQL_SCHEMA_PATH`
- Database unreachable: Check `DATABASE_URL` and network connectivity
- Port already in use: Change `FRAISEQL_PORT`

### High Latency

1. Check database performance:

```sql
SELECT * FROM pg_stat_statements ORDER BY mean_time DESC LIMIT 10;
```text

1. Monitor connection pool:

```bash
curl http://localhost:8000/health | jq .database.connection_pool
```text

1. Check query complexity:

- Simplify queries
- Add pagination
- Use field selection (don't fetch all fields)

### Memory Leak

1. Check connection pool isn't unbounded
2. Verify schema doesn't have circular references
3. Monitor with: `docker stats fraiseql`

## Scaling

### Horizontal Scaling

Add more replicas:

```yaml
# Kubernetes
replicas: 5

# Docker Swarm
docker service scale fraiseql=5
```text

Connection pool scales across instances (no coordination needed).

### Vertical Scaling

Increase resources:

```yaml
resources:
  limits:
    memory: 1Gi
    cpu: 1000m
```text

Typically not needed for GraphQL execution.

## Rollback

### Kubernetes

```bash
kubectl rollout history deployment/fraiseql-server
kubectl rollout undo deployment/fraiseql-server --to-revision=2
```text

### Docker Swarm

```bash
docker service rollback fraiseql-server
```text

### Manual

Keep previous v2 image tags:

```bash
docker run -p 8000:8000 fraiseql-server:v2.0.0-alpha.0  # Previous v2 version
# Note: v1 versions are NOT compatible with v2 schemas
```text

## Next Steps

- See [HTTP_SERVER.md](../reference/api/HTTP_SERVER.md) for server configuration options
- See [GRAPHQL_API.md](../reference/api/GRAPHQL_API.md) for API specification
- See [examples/](../../examples/) for query examples
