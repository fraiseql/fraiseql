# Deployment Guide

## Overview

FraiseQL is designed to be deployed in various environments, from simple Docker containers to complex Kubernetes clusters. This guide covers all deployment options with production-ready configurations.

## Deployment Options

### 🐳 [Docker Deployment](./docker.md)
Simple containerized deployment with Docker and Docker Compose. Perfect for:

- Development environments
- Small to medium production workloads
- Single-server deployments
- Quick prototyping

**Estimated Cost**: $50-200/month

### ☸️ [Kubernetes Deployment](./kubernetes.md)
Scalable orchestration with Kubernetes and Helm charts. Ideal for:

- Large-scale production deployments
- Multi-region setups
- Auto-scaling requirements
- Enterprise environments

**Estimated Cost**: $500-2000+/month

### ☁️ Cloud Platform Deployments

#### [AWS Deployment](./aws.md)

- ECS Fargate for serverless containers
- RDS PostgreSQL for managed database
- Application Load Balancer
- CloudWatch monitoring

**Estimated Cost**: $200-1000/month

#### [Google Cloud Platform](./gcp.md)

- Cloud Run for serverless deployment
- Cloud SQL for PostgreSQL
- Load Balancing
- Cloud Monitoring

**Estimated Cost**: $150-800/month

#### [Heroku](./heroku.md)

- One-click deployment
- Managed PostgreSQL
- Automatic SSL
- Simple scaling

**Estimated Cost**: $50-500/month

## Quick Start

### Minimal Production Setup

```bash
# Clone the repository
git clone https://github.com/your-org/fraiseql.git
cd fraiseql

# Create environment file
cat > .env << EOF
DATABASE_URL=postgresql://user:pass@localhost:5432/fraiseql
SECRET_KEY=$(openssl rand -hex 32)
FRAISEQL_MODE=production
EOF

# Build and run with Docker
docker-compose up -d
```

## Architecture Overview

```mermaid
graph TB
    subgraph "Client Layer"
        WEB[Web App]
        MOB[Mobile App]
        API[API Client]
    end

    subgraph "Load Balancer"
        LB[Nginx/ALB/Cloud LB]
    end

    subgraph "Application Layer"
        APP1[FraiseQL Instance 1]
        APP2[FraiseQL Instance 2]
        APP3[FraiseQL Instance N]
    end

    subgraph "Cache Layer"
        REDIS[Redis Cache]
    end

    subgraph "Database Layer"
        PRIMARY[PostgreSQL Primary]
        REPLICA[PostgreSQL Replica]
    end

    WEB --> LB
    MOB --> LB
    API --> LB

    LB --> APP1
    LB --> APP2
    LB --> APP3

    APP1 --> REDIS
    APP2 --> REDIS
    APP3 --> REDIS

    APP1 --> PRIMARY
    APP2 --> PRIMARY
    APP3 --> PRIMARY

    PRIMARY --> REPLICA
```

## System Requirements

### Minimum Requirements

- **CPU**: 2 cores
- **RAM**: 2GB
- **Storage**: 10GB SSD
- **PostgreSQL**: 14+
- **Python**: 3.11+

### Recommended Production Setup

- **CPU**: 4+ cores
- **RAM**: 8GB+
- **Storage**: 100GB+ SSD
- **PostgreSQL**: 15+ with replication
- **Redis**: For caching
- **Load Balancer**: For high availability

## Environment Variables

### Required Variables
```bash
DATABASE_URL=postgresql://user:password@host:5432/database
SECRET_KEY=your-secret-key-here
FRAISEQL_MODE=production
```

### Optional Variables
```bash
# Redis for caching
REDIS_URL=redis://localhost:6379/0

# Performance
MAX_CONNECTIONS=100
STATEMENT_TIMEOUT=30000
QUERY_COMPLEXITY_LIMIT=1000

# Security
# FRAISEQL_CORS_ENABLED=true  # Only if serving browsers directly
# FRAISEQL_CORS_ORIGINS=https://example.com
RATE_LIMIT_PER_MINUTE=100
JWT_EXPIRATION=3600

# Monitoring
PROMETHEUS_ENABLED=true
METRICS_PORT=9090
LOG_LEVEL=INFO
```

## PostgreSQL Configuration

### Recommended Settings
```sql
-- Connection pooling
max_connections = 200
shared_buffers = 256MB

-- Performance
effective_cache_size = 1GB
maintenance_work_mem = 128MB
checkpoint_completion_target = 0.9
wal_buffers = 16MB

-- Query optimization
default_statistics_target = 100
random_page_cost = 1.1
effective_io_concurrency = 200
```

### Required Extensions
```sql
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";
CREATE EXTENSION IF NOT EXISTS "btree_gist";
```

## Security Considerations

### Essential Security Measures

1. **Use HTTPS/TLS** for all connections
2. **Enable rate limiting** to prevent abuse
3. **Implement authentication** (JWT, OAuth2)
4. **Use secrets management** (never hardcode)
5. **Regular security updates**
6. **Database connection encryption**
7. **Input validation and sanitization**

### Network Security
```yaml
# Example network policy
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: fraiseql-network-policy
spec:
  podSelector:
    matchLabels:
      app: fraiseql
  ingress:

  - from:

    - podSelector:
        matchLabels:
          app: nginx
    ports:

    - protocol: TCP
      port: 8000
```

## Monitoring & Observability

### Key Metrics to Monitor

- **Query response time** (P50, P95, P99)
- **Database connection pool usage**
- **Cache hit rate**
- **Error rate**
- **CPU and memory usage**
- **Request throughput**

### Recommended Stack

- **Metrics**: Prometheus + Grafana
- **Logging**: ELK Stack or Loki
- **Tracing**: Jaeger or Zipkin
- **Error Tracking**: Sentry
- **Uptime**: Pingdom or UptimeRobot

## Scaling Strategies

### Horizontal Scaling

- Add more application instances
- Use load balancing
- Implement connection pooling
- Cache frequently accessed data

### Vertical Scaling

- Increase CPU/RAM for instances
- Optimize database queries
- Use read replicas
- Implement query complexity limits

## Deployment Checklist

Before going to production, ensure:

- [ ] Production mode enabled
- [ ] SSL/TLS configured
- [ ] Database backups scheduled
- [ ] Monitoring configured
- [ ] Health checks implemented
- [ ] Rate limiting enabled
- [ ] Error handling tested
- [ ] Load testing completed
- [ ] Security scan passed
- [ ] Documentation updated

## Next Steps

1. Choose your deployment platform:

   - [Docker](./docker.md) for simplicity
   - [Kubernetes](./kubernetes.md) for scale
   - [Cloud platforms](./aws.md) for managed services

2. Review the [Production Checklist](./production-checklist.md)

3. Set up [Monitoring](./monitoring.md) and [Scaling](./scaling.md)

## Support

- **Documentation**: [https://fraiseql.dev/docs](https://fraiseql.dev/docs)
- **GitHub Issues**: [https://github.com/your-org/fraiseql/issues](https://github.com/your-org/fraiseql/issues)
- **Community Forum**: [https://forum.fraiseql.dev](https://forum.fraiseql.dev)
