# Deployment Documentation

Deploy FraiseQL applications to Docker, Kubernetes, cloud platforms, and traditional hosting.

## Quick Start Deployment

### Docker Deployment

**Minimal Docker Setup:**

```dockerfile
# Dockerfile
FROM python:3.10-slim

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    libpq-dev gcc && \
    rm -rf /var/lib/apt/lists/*

# Install Python dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application
COPY . .

# Run application
CMD ["uvicorn", "app:app", "--host", "0.0.0.0", "--port", "8000"]
```

**Docker Compose:**

```yaml
# docker-compose.yml
version: '3.8'

services:
  app:
    build: .
    ports:
      - "8000:8000"
    environment:
      - DATABASE_URL=postgresql://user:password@db:5432/fraiseql
      - ENVIRONMENT=production
    depends_on:
      - db
      - pgbouncer

  db:
    image: postgres:16
    environment:
      - POSTGRES_DB=fraiseql
      - POSTGRES_USER=user
      - POSTGRES_PASSWORD=password
    volumes:
      - postgres_data:/var/lib/postgresql/data

  pgbouncer:
    image: pgbouncer/pgbouncer
    environment:
      - DATABASE_URL=postgresql://user:password@db:5432/fraiseql
    ports:
      - "6432:6432"

volumes:
  postgres_data:
```

---

## Complete Deployment Templates

### Docker Compose (Production-Ready)

**File**: `deployment/docker-compose.prod.yml`

Includes:
- ✅ FraiseQL application (3 replicas with health checks)
- ✅ PostgreSQL 16 with optimized configuration
- ✅ PgBouncer connection pooling
- ✅ Grafana with pre-configured dashboards
- ✅ Nginx reverse proxy with SSL support
- ✅ Resource limits and restart policies

**Deploy:**

```bash
cd deployment
cp .env.example .env
# Edit .env with production values

docker-compose -f docker-compose.prod.yml up -d

# Verify
docker-compose ps
curl http://localhost:8000/health
```

**[View complete template →](../../deployment/docker-compose.prod.yml)**

---

## Kubernetes Deployment

**Basic Kubernetes Manifests:**

```yaml
# deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-app
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
        image: your-registry/fraiseql:latest
        ports:
        - containerPort: 8000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: database-url
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health/ready
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 5
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
```

---

### Kubernetes (Production-Ready)

**Files**:
- `deployment/k8s/deployment.yaml` - Application deployment, service, HPA, ingress
- `deployment/k8s/postgres.yaml` - PostgreSQL StatefulSet with persistent storage

Includes:
- ✅ Horizontal Pod Autoscaler (3-10 replicas)
- ✅ Resource requests and limits
- ✅ Liveness, readiness, and startup probes
- ✅ Ingress with TLS (Let's Encrypt)
- ✅ PostgreSQL StatefulSet with persistent volume
- ✅ Secrets management
- ✅ ConfigMaps for environment configuration

**Deploy:**

```bash
# Apply manifests
kubectl apply -f deployment/k8s/postgres.yaml
kubectl apply -f deployment/k8s/deployment.yaml

# Verify deployment
kubectl get pods -n fraiseql
kubectl logs -f deployment/fraiseql-app -n fraiseql

# Check autoscaling
kubectl get hpa -n fraiseql
```

**[View complete templates →](../../deployment/k8s/)**

---

### Production Checklist

Before deploying these templates:

#### Secrets & Configuration
- [ ] Update `.env` or Kubernetes secrets with strong passwords
- [ ] Generate unique `SECRET_KEY` (32+ random characters)
- [ ] Configure `ALLOWED_ORIGINS` for your domain
- [ ] Set up error notification email

#### Infrastructure
- [ ] Provision persistent storage (50GB+ for PostgreSQL)
- [ ] Configure backup strategy (pg_dump scheduled)
- [ ] Set up monitoring (import Grafana dashboards)
- [ ] Configure DNS for your domain

#### Security
- [ ] Enable TLS/SSL certificates (Let's Encrypt or ACM)
- [ ] Configure firewall rules (block PostgreSQL port externally)
- [ ] Enable Row-Level Security in PostgreSQL
- [ ] Review CORS configuration

#### Performance
- [ ] Tune PostgreSQL configuration for your hardware
- [ ] Configure PgBouncer pool sizes
- [ ] Set appropriate resource limits
- [ ] Enable APQ with PostgreSQL backend

**[Complete production checklist →](../production/README.md#production-checklist)**

---

## Cloud Platform Guides

### AWS Deployment

**Recommended Stack:**
- **Compute**: ECS Fargate or EKS
- **Database**: RDS PostgreSQL (t3.medium or larger)
- **Connection Pooling**: RDS Proxy or PgBouncer sidecar
- **Load Balancer**: Application Load Balancer (ALB)
- **Secrets**: AWS Secrets Manager

**Quick Deploy with ECS:**

```bash
# 1. Create ECR repository
aws ecr create-repository --repository-name fraiseql-app

# 2. Build and push Docker image
docker build -t fraiseql-app .
docker tag fraiseql-app:latest ${AWS_ACCOUNT_ID}.dkr.ecr.${AWS_REGION}.amazonaws.com/fraiseql-app:latest
docker push ${AWS_ACCOUNT_ID}.dkr.ecr.${AWS_REGION}.amazonaws.com/fraiseql-app:latest

# 3. Deploy with ECS (use CloudFormation or Terraform template)
```

**AWS-specific considerations:**
- Use RDS PostgreSQL 16+ with pgBouncer via RDS Proxy
- Enable Multi-AZ for high availability
- Use ElastiCache for PostgreSQL if needed (though FraiseQL caching in PostgreSQL is often sufficient)

---

### GCP Deployment

**Recommended Stack:**
- **Compute**: Cloud Run or GKE
- **Database**: Cloud SQL for PostgreSQL
- **Connection Pooling**: Cloud SQL Proxy
- **Load Balancer**: Cloud Load Balancing

**Quick Deploy with Cloud Run:**

```bash
# 1. Build and push to Google Container Registry
gcloud builds submit --tag gcr.io/${PROJECT_ID}/fraiseql-app

# 2. Deploy to Cloud Run
gcloud run deploy fraiseql-app \
  --image gcr.io/${PROJECT_ID}/fraiseql-app \
  --platform managed \
  --region us-central1 \
  --allow-unauthenticated \
  --set-env-vars DATABASE_URL=${DATABASE_URL}
```

**GCP-specific considerations:**
- Use Cloud SQL with connection pooling (built-in)
- Enable automatic scaling (Cloud Run handles this)
- Use Secret Manager for credentials

---

### Azure Deployment

**Recommended Stack:**
- **Compute**: Container Instances or AKS
- **Database**: Azure Database for PostgreSQL - Flexible Server
- **Connection Pooling**: PgBouncer sidecar
- **Load Balancer**: Azure Load Balancer

**Quick Deploy with Container Instances:**

```bash
# 1. Create Azure Container Registry
az acr create --name fraiseqlregistry --resource-group myResourceGroup --sku Basic

# 2. Build and push
az acr build --registry fraiseqlregistry --image fraiseql-app:latest .

# 3. Deploy container instance
az container create \
  --resource-group myResourceGroup \
  --name fraiseql-app \
  --image fraiseqlregistry.azurecr.io/fraiseql-app:latest \
  --dns-name-label fraiseql-app \
  --ports 8000 \
  --environment-variables DATABASE_URL=${DATABASE_URL}
```

---

## Traditional Hosting (VPS/Dedicated Servers)

### systemd Service Setup

```ini
# /etc/systemd/system/fraiseql.service
[Unit]
Description=FraiseQL GraphQL API
After=network.target postgresql.service

[Service]
Type=notify
User=fraiseql
Group=fraiseql
WorkingDirectory=/opt/fraiseql
Environment="PATH=/opt/fraiseql/venv/bin"
EnvironmentFile=/opt/fraiseql/.env
ExecStart=/opt/fraiseql/venv/bin/uvicorn app:app --host 0.0.0.0 --port 8000 --workers 4
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

**Enable and start:**

```bash
sudo systemctl daemon-reload
sudo systemctl enable fraiseql
sudo systemctl start fraiseql
```

### Nginx Reverse Proxy

```nginx
# /etc/nginx/sites-available/fraiseql
server {
    listen 80;
    server_name api.example.com;

    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # WebSocket support for subscriptions
    location /graphql {
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

---

## Environment Configuration

### Environment Variables

```bash
# .env.production
DATABASE_URL=postgresql://user:password@db:5432/fraiseql
ENVIRONMENT=production
DEBUG=false

# APQ Configuration
APQ_STORAGE_BACKEND=postgresql
APQ_STORAGE_SCHEMA=apq_cache

# Security
ALLOWED_ORIGINS=https://app.example.com,https://www.example.com
SECRET_KEY=your-secret-key-here

# Monitoring
ENABLE_ERROR_TRACKING=true
ERROR_NOTIFICATION_EMAIL=alerts@example.com
```

### Secrets Management Best Practices

- ✅ Use cloud provider secrets managers (AWS Secrets Manager, GCP Secret Manager, Azure Key Vault)
- ✅ Never commit `.env` files to version control
- ✅ Rotate database credentials regularly
- ✅ Use least-privilege database roles

---

## Deployment Checklist

See **[Production Checklist](../production/README.md#production-checklist)** for complete pre-deployment verification.

---

## Scaling Strategies

### Horizontal Scaling

FraiseQL applications are **stateless** and scale horizontally:

```yaml
# Kubernetes HPA
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: fraiseql-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql-app
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

### Database Connection Pooling

**Critical for production**: Use PgBouncer or similar:

```ini
# pgbouncer.ini
[databases]
fraiseql = host=db port=5432 dbname=fraiseql

[pgbouncer]
pool_mode = transaction
max_client_conn = 1000
default_pool_size = 20
```

**Pool sizing formula**: `(2 × CPU cores) + effective_spindle_count`

See **[Production Deployment Guide](../production/deployment.md#scaling-strategies)** for details.

---

## Support & Additional Resources

- **[Production Guide](../production/)** - Monitoring, security, observability
- **[Security Policy](../../SECURITY.md)** - Security best practices
- **[Health Checks](../production/health-checks.md)** - Liveness/readiness probes
- **[Troubleshooting](../guides/troubleshooting.md)** - Common deployment issues

**Need help?** Open an issue at [GitHub Issues](../issues)
