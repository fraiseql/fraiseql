# Production Deployment: Starlette

**Version**: 2.0.0+
**Reading Time**: 30 minutes
**Audience**: DevOps engineers, backend developers
**Prerequisites**: Completed [Configuration Guide](./02-configuration.md)

---

## Overview

This guide covers deploying your Starlette GraphQL server to production:
- ✅ Docker containerization
- ✅ Kubernetes deployment
- ✅ Cloud platform deployment (AWS, GCP, Azure)
- ✅ Health checks and monitoring
- ✅ Scaling strategies
- ✅ Common issues and solutions

---

## Building for Production

### Python Environment Setup

**Install production dependencies**:
```bash
# Create requirements.txt with all dependencies
pip freeze > requirements.txt

# Or explicitly list
cat > requirements.txt << EOF
fraiseql==2.0.0
starlette==0.36.0
uvicorn==0.28.0
pydantic==2.5.0
sqlalchemy==2.0.0
psycopg[binary]==3.17.0
python-dotenv==1.0.0
gunicorn==21.2.0
EOF
```

### Optimize for Production

```bash
# Use production ASGI server
pip install gunicorn

# Run with production settings
gunicorn \
  --workers 4 \
  --worker-class uvicorn.workers.UvicornWorker \
  --bind 0.0.0.0:8000 \
  --timeout 120 \
  --access-logfile - \
  main:app
```

### Performance Comparison

```
Development (uvicorn main:app):  ~100 req/s
Production (gunicorn + uvicorn):  ~1000-5000 req/s (4-8 workers)
Production (gunicorn + uvicorn, tuned):  ~10000 req/s (8-16 workers)
```

---

## Docker Deployment

### Multi-Stage Dockerfile

**Optimized for size and security**:
```dockerfile
# Stage 1: Build
FROM python:3.13-slim as builder

WORKDIR /app

# Copy requirements
COPY requirements.txt .

# Install dependencies
RUN pip install --user --no-cache-dir -r requirements.txt

# Stage 2: Runtime
FROM python:3.13-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Python packages from builder
COPY --from=builder /root/.local /root/.local

# Copy application code
COPY . .

# Create non-root user
RUN useradd -m appuser
USER appuser

# Set PATH for user-installed packages
ENV PATH=/root/.local/bin:$PATH

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health')" || exit 1

# Run application
CMD ["gunicorn", \
     "--workers", "4", \
     "--worker-class", "uvicorn.workers.UvicornWorker", \
     "--bind", "0.0.0.0:8000", \
     "--timeout", "120", \
     "main:app"]
```

### Lean Alpine Dockerfile

**For minimal size**:
```dockerfile
# Stage 1: Build
FROM python:3.13-alpine as builder

WORKDIR /app

COPY requirements.txt .

# Install build dependencies and packages
RUN apk add --no-cache gcc musl-dev postgresql-dev && \
    pip install --user --no-cache-dir -r requirements.txt

# Stage 2: Runtime (very small)
FROM python:3.13-alpine

RUN apk add --no-cache postgresql-client

WORKDIR /app

COPY --from=builder /root/.local /root/.local
COPY . .

RUN adduser -D appuser
USER appuser

ENV PATH=/root/.local/bin:$PATH

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost:8000/health || exit 1

CMD ["gunicorn", \
     "--workers", "4", \
     "--worker-class", "uvicorn.workers.UvicornWorker", \
     "--bind", "0.0.0.0:8000", \
     "main:app"]
```

**Size comparison**:
- Debian-based: ~400MB
- Alpine-based: ~150MB

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
        - name: STARLETTE_HOST
          value: "0.0.0.0"
        - name: STARLETTE_PORT
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
          "name": "STARLETTE_HOST",
          "value": "0.0.0.0"
        },
        {
          "name": "STARLETTE_PORT",
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

```python
from starlette.responses import JSONResponse
from starlette.requests import Request
from datetime import datetime
import os

async def health_check(request: Request):
    """Health check endpoint"""
    try:
        # Check database connectivity
        db_status = await check_database()

        return JSONResponse({
            "status": "healthy",
            "timestamp": datetime.utcnow().isoformat(),
            "version": os.environ.get("APP_VERSION", "unknown"),
            "database": db_status,
        })
    except Exception as e:
        return JSONResponse(
            {
                "status": "unhealthy",
                "error": str(e),
            },
            status_code=503
        )

async def check_database():
    """Check database connectivity"""
    try:
        # Try a simple query
        async with db_pool.acquire() as conn:
            await conn.fetchval("SELECT 1")
        return "connected"
    except Exception:
        return "disconnected"

# Add route
app.add_route("/health", health_check, methods=["GET"])
```

### Monitoring with Prometheus

```python
from prometheus_client import Counter, Histogram, start_http_server
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
import time

# Create metrics
request_count = Counter(
    'graphql_requests_total',
    'Total GraphQL requests',
    ['method', 'endpoint']
)

request_duration = Histogram(
    'graphql_request_duration_seconds',
    'GraphQL request duration in seconds',
    ['method', 'endpoint']
)

class MetricsMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        start = time.time()

        response = await call_next(request)

        duration = time.time() - start

        request_count.labels(
            method=request.method,
            endpoint=request.url.path
        ).inc()

        request_duration.labels(
            method=request.method,
            endpoint=request.url.path
        ).observe(duration)

        return response

# Add middleware
app.add_middleware(MetricsMiddleware)

# Start Prometheus metrics server
if __name__ == "__main__":
    start_http_server(8001)  # Metrics on port 8001
    # Run app on port 8000
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

### Issue 4: "Slow Startup"

**Cause**: Cold start, database migrations, or warm-up

**Solution**:
```python
# Increase startup grace period
readinessProbe:
  initialDelaySeconds: 15  # Increase from 5

# Or run warm-up in startup event
@app.on_event("startup")
async def startup():
    # Warm up database connections
    async with db_pool.acquire() as conn:
        await conn.fetchval("SELECT 1")
```

---

## Scaling Strategies

### Horizontal Scaling

```yaml
# Auto-scale based on CPU
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
```

### Vertical Scaling

```yaml
# Increase per-instance resources
resources:
  requests:
    memory: "512Mi"
    cpu: "250m"
  limits:
    memory: "2Gi"
    cpu: "1000m"
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
- **Back to Configuration?** → [Configuration](./02-configuration.md)

---

**Your API is now in production!** Monitor and optimize from there. 🚀
