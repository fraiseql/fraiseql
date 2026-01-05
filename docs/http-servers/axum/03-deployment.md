# Production Deployment: Axum

**Version**: 2.0.0+
**Reading Time**: 30 minutes
**Audience**: DevOps engineers, backend developers
**Prerequisites**: Completed [Configuration Guide](./02-configuration.md)

---

## Overview

This guide covers deploying your Axum GraphQL server to production:
- ✅ Docker containerization
- ✅ Kubernetes deployment
- ✅ Cloud platform deployment (AWS, GCP, Azure)
- ✅ Health checks and monitoring
- ✅ Scaling strategies
- ✅ Common issues and solutions

---

## Building for Production

### Rust Release Build

**Compile optimized binary**:
```bash
# Build optimized release binary
cargo build --release

# Binary location: target/release/my-graphql-api
# Size: typically 20-100MB (depending on features)
```

**Performance difference**:
```
Debug build:   ~0.5-1ms per query
Release build: ~0.1-0.5ms per query (2-5x faster)
```

### Minimize Binary Size

**Use strip tool** (optional):
```bash
# Strip debug symbols
strip target/release/my-graphql-api

# Reduces size by ~50%
# From 100MB → 50MB
```

---

## Docker Deployment

### Multi-Stage Dockerfile

**Optimized for size**:
```dockerfile
# Stage 1: Build
FROM rust:1.75 as builder

WORKDIR /app

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./

# Build dependencies (cached layer)
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy source and build application
COPY src ./src
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/my-graphql-api /app/

# Create non-root user
RUN useradd -m appuser
USER appuser

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Run application
CMD ["./my-graphql-api"]
```

### Lean Alpine Dockerfile

**For minimal size**:
```dockerfile
# Stage 1: Build
FROM rust:1.75-alpine as builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

COPY src ./src
RUN cargo build --release

# Stage 2: Runtime (very small)
FROM alpine:3.18

RUN apk add --no-cache ca-certificates libpq

WORKDIR /app
COPY --from=builder /app/target/release/my-graphql-api /app/

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost:8000/health || exit 1

CMD ["./my-graphql-api"]
```

**Size comparison**:
- Debian-based: ~200MB
- Alpine-based: ~100MB

### Build and Push

```bash
# Build image
docker build -t my-graphql-api:latest .
docker build -t my-graphql-api:v1.0.0 .

# Push to registry
docker push my-registry/my-graphql-api:latest
docker push my-registry/my-graphql-api:v1.0.0

# Run locally to test
docker run -p 8000:8000 \
  -e DATABASE_URL="postgresql://..." \
  -e JWT_SECRET="..." \
  my-graphql-api:latest
```

---

## Kubernetes Deployment

### Basic Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: graphql-api
  labels:
    app: graphql-api
spec:
  replicas: 3
  selector:
    matchLabels:
      app: graphql-api
  template:
    metadata:
      labels:
        app: graphql-api
    spec:
      containers:
      - name: api
        image: my-registry/my-graphql-api:v1.0.0
        ports:
        - containerPort: 8000
          name: http
        env:
        - name: AXUM_HOST
          value: "0.0.0.0"
        - name: AXUM_PORT
          value: "8000"
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-secret
              key: connection-string
        - name: JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: api-secret
              key: jwt-secret
        resources:
          requests:
            memory: "128Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 10
---
apiVersion: v1
kind: Service
metadata:
  name: graphql-api-service
spec:
  selector:
    app: graphql-api
  ports:
  - port: 80
    targetPort: 8000
    protocol: TCP
  type: LoadBalancer
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: graphql-api-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: graphql-api
  minReplicas: 3
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
```

### Deploy to Kubernetes

```bash
# Create namespace
kubectl create namespace production

# Create secrets
kubectl create secret generic db-secret \
  --from-literal=connection-string='postgresql://...' \
  -n production

kubectl create secret generic api-secret \
  --from-literal=jwt-secret='...' \
  -n production

# Deploy
kubectl apply -f deployment.yaml -n production

# Check deployment
kubectl get pods -n production
kubectl logs -f deployment/graphql-api -n production

# Scale manually (if needed)
kubectl scale deployment graphql-api --replicas=5 -n production

# Check status
kubectl rollout status deployment/graphql-api -n production
```

---

## Cloud Platform Deployment

### AWS: Elastic Container Service (ECS)

**Task definition**:
```json
{
  "family": "graphql-api",
  "networkMode": "awsvpc",
  "requiresCompatibilities": ["FARGATE"],
  "cpu": "256",
  "memory": "512",
  "containerDefinitions": [
    {
      "name": "api",
      "image": "123456789.dkr.ecr.us-east-1.amazonaws.com/my-graphql-api:latest",
      "portMappings": [
        {
          "containerPort": 8000,
          "protocol": "tcp"
        }
      ],
      "environment": [
        {
          "name": "AXUM_HOST",
          "value": "0.0.0.0"
        },
        {
          "name": "AXUM_PORT",
          "value": "8000"
        }
      ],
      "secrets": [
        {
          "name": "DATABASE_URL",
          "valueFrom": "arn:aws:secretsmanager:us-east-1:123456789:secret:db-url:connection:0"
        },
        {
          "name": "JWT_SECRET",
          "valueFrom": "arn:aws:secretsmanager:us-east-1:123456789:secret:jwt-secret:token:0"
        }
      ],
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group": "/ecs/graphql-api",
          "awslogs-region": "us-east-1",
          "awslogs-stream-prefix": "ecs"
        }
      },
      "healthCheck": {
        "command": ["CMD-SHELL", "curl -f http://localhost:8000/health || exit 1"],
        "interval": 30,
        "timeout": 10,
        "retries": 3
      }
    }
  ]
}
```

**Deploy with Terraform**:
```hcl
resource "aws_ecs_service" "graphql_api" {
  name            = "graphql-api"
  cluster         = aws_ecs_cluster.main.id
  task_definition = aws_ecs_task_definition.graphql_api.arn
  desired_count   = 3
  launch_type     = "FARGATE"

  network_configuration {
    subnets          = aws_subnet.private.*.id
    security_groups  = [aws_security_group.ecs.id]
    assign_public_ip = false
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.graphql_api.arn
    container_name   = "api"
    container_port   = 8000
  }

  depends_on = [
    aws_lb_listener.graphql_api,
    aws_iam_role_policy.ecs_task_execution_role_policy
  ]
}
```

### GCP: Cloud Run

**Deploy**:
```bash
# Build image
gcloud builds submit --tag gcr.io/PROJECT_ID/my-graphql-api

# Deploy
gcloud run deploy my-graphql-api \
  --image gcr.io/PROJECT_ID/my-graphql-api \
  --platform managed \
  --region us-central1 \
  --memory 512Mi \
  --cpu 1 \
  --set-env-vars DATABASE_URL=postgresql://... \
  --set-env-vars JWT_SECRET=... \
  --allow-unauthenticated
```

### Azure: Container Instances

**Deploy**:
```bash
# Create container
az container create \
  --resource-group myResourceGroup \
  --name graphql-api \
  --image myregistry.azurecr.io/my-graphql-api:latest \
  --cpu 1 \
  --memory 0.5 \
  --registry-login-server myregistry.azurecr.io \
  --registry-username <username> \
  --registry-password <password> \
  --environment-variables DATABASE_URL=postgresql://... \
  --dns-name-label graphql-api \
  --ports 8000
```

---

## Health Checks & Monitoring

### Health Check Endpoint

```rust
async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now(),
        "version": env!("CARGO_PKG_VERSION"),
        "database": check_database().await,
    }))
}

async fn check_database() -> String {
    // Check database connectivity
    match pool.get().await {
        Ok(_) => "connected".to_string(),
        Err(_) => "disconnected".to_string(),
    }
}

let app = Router::new()
    .route("/health", get(health_check));
```

### Monitoring with Prometheus

```rust
use prometheus::{Counter, Histogram, Registry};

let request_counter = Counter::new("graphql_requests_total", "Total requests").unwrap();
let request_duration = Histogram::new("graphql_request_duration_seconds", "Request duration").unwrap();

async fn metrics_middleware<B>(
    req: Request<B>,
    next: Next,
) -> Response {
    request_counter.inc();

    let start = std::time::Instant::now();
    let response = next.run(req).await;

    let duration = start.elapsed().as_secs_f64();
    request_duration.observe(duration);

    response
}

let app = Router::new()
    .route("/metrics", get(|| async {
        // Return Prometheus metrics
    }));
```

---

## Performance Optimization

### Connection Pool Sizing

```rust
// Database connections
const MIN_POOL_SIZE: u32 = 5;
const MAX_POOL_SIZE: u32 = 20;

// Formula: (number of cores × 2) + max overflow
// For 4-core server: (4 × 2) + 4 = 12 connections

let pool = PgPoolOptions::new()
    .min_connections(MIN_POOL_SIZE)
    .max_connections(MAX_POOL_SIZE)
    .acquire_timeout(Duration::from_secs(30))
    .connect(&database_url)
    .await?;
```

### Worker Threads

```rust
let num_workers = num_cpus::get();  // Use all CPU cores

#[tokio::main(worker_threads = 4)]
async fn main() {
    // 4 worker threads
}
```

---

## Common Deployment Issues

### Issue 1: "Connection Timeout to Database"

**Cause**: Database not reachable from container

**Solution**:
```bash
# Check database URL
echo $DATABASE_URL

# Test connection locally
psql $DATABASE_URL

# Check Kubernetes network policies
kubectl get networkpolicies

# Check security groups (AWS)
aws ec2 describe-security-groups
```

### Issue 2: "Out of Memory"

**Cause**: Memory limit too low

**Solution**:
```yaml
# Increase memory
resources:
  limits:
    memory: "1Gi"    # Increase from 512Mi
  requests:
    memory: "512Mi"
```

### Issue 3: "Health Check Failing"

**Cause**: Health endpoint not responding

**Solution**:
```bash
# Test health locally
curl http://localhost:8000/health

# Check logs
kubectl logs -f deployment/graphql-api
docker logs container-id

# Increase timeout
livenessProbe:
  initialDelaySeconds: 30  # Increase from 10
```

### Issue 4: "Slow Queries After Deployment"

**Cause**: Connection pool too small or cold start

**Solution**:
```rust
// Increase pool size
.max_connections(50)

// Warm up pool on startup
for _ in 0..10 {
    pool.acquire().await?;
}
```

---

## Rollback Strategy

### Quick Rollback (Kubernetes)

```bash
# Check rollout history
kubectl rollout history deployment/graphql-api

# Rollback to previous version
kubectl rollout undo deployment/graphql-api

# Rollback to specific revision
kubectl rollout undo deployment/graphql-api --to-revision=2

# Check status
kubectl rollout status deployment/graphql-api
```

### Blue-Green Deployment

```yaml
# Deploy new version alongside old
apiVersion: apps/v1
kind: Deployment
metadata:
  name: graphql-api-v2
spec:
  # ... deployment config ...
---
# Switch traffic gradually
apiVersion: v1
kind: Service
metadata:
  name: graphql-api
spec:
  selector:
    app: graphql-api-v2  # Change from v1 to v2
```

---

## Verification Checklist

Before going live:

- [ ] Docker image builds successfully
- [ ] Health check endpoint responds (http://localhost:8000/health)
- [ ] Metrics endpoint works (/metrics)
- [ ] Database connection working
- [ ] Environment variables set correctly
- [ ] Security headers present
- [ ] CORS configured correctly
- [ ] Rate limiting working
- [ ] Logging enabled
- [ ] Monitoring configured
- [ ] Backup/rollback plan ready
- [ ] Load tested

---

## Next Steps

- **Performance tuning needed?** → [Performance Tuning](./04-performance.md)
- **Something not working?** → [Troubleshooting](./05-troubleshooting.md)
- **Back to Getting Started?** → [Getting Started](./01-getting-started.md)

---

**Your API is now in production!** Monitor and optimize from there. 🚀
