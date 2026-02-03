# FraiseQL Federation Deployment Guide

Production-ready deployment of FraiseQL federation across multiple clouds.

## Table of Contents

1. [Single Cloud Deployment](#single-cloud-deployment)
2. [Multi-Cloud Deployment](#multi-cloud-deployment)
3. [Performance Optimization](#performance-optimization)
4. [Monitoring & Observability](#monitoring--observability)
5. [Troubleshooting](#troubleshooting)

---

## Single Cloud Deployment

### AWS Deployment

**Setup with AWS RDS PostgreSQL and ECS:**

```bash
# 1. Create RDS instance
aws rds create-db-instance \
  --db-instance-identifier fraiseql-users \
  --db-instance-class db.t3.micro \
  --engine postgres \
  --master-username postgres \
  --master-user-password <password>

# 2. Get endpoint
aws rds describe-db-instances \
  --db-instance-identifier fraiseql-users \
  --query 'DBInstances[0].Endpoint.Address'

# 3. Create ECR repository
aws ecr create-repository --repository-name fraiseql-users

# 4. Build and push Docker image
docker build -t fraiseql-users:latest users-service/
aws ecr get-login-password | docker login --username AWS --password-stdin <account>.dkr.ecr.us-east-1.amazonaws.com
docker tag fraiseql-users:latest <account>.dkr.ecr.us-east-1.amazonaws.com/fraiseql-users:latest
docker push <account>.dkr.ecr.us-east-1.amazonaws.com/fraiseql-users:latest

# 5. Deploy to ECS
aws ecs create-service \
  --cluster fraiseql \
  --service-name fraiseql-users \
  --task-definition fraiseql-users:1 \
  --desired-count 2 \
  --load-balancers targetGroupArn=arn:aws:elasticloadbalancing:...
```

**Expected Performance:**
- Single query: <5ms
- Batch 100: ~10ms
- Cross-AZ: +5-10ms latency

---

### GCP Deployment

**Setup with Cloud SQL PostgreSQL and Cloud Run:**

```bash
# 1. Create Cloud SQL instance
gcloud sql instances create fraiseql-users \
  --database-version=POSTGRES_16 \
  --tier=db-f1-micro \
  --region=us-central1

# 2. Get connection string
gcloud sql instances describe fraiseql-users \
  --format='value(connectionName)'

# 3. Build and push image
gcloud builds submit --tag us-central1-docker.pkg.dev/PROJECT/fraiseql/users-service

# 4. Deploy to Cloud Run
gcloud run deploy fraiseql-users \
  --image us-central1-docker.pkg.dev/PROJECT/fraiseql/users-service \
  --platform managed \
  --region us-central1 \
  --set-env-vars DATABASE_URL=postgresql://...
```

**Expected Performance:**
- Single query: <5ms
- Batch 100: ~10ms
- Cross-region: +10-20ms latency

---

### Azure Deployment

**Setup with Azure Database for PostgreSQL and Container Instances:**

```bash
# 1. Create PostgreSQL server
az postgres server create \
  --resource-group fraiseql \
  --name fraiseql-users \
  --location eastus \
  --admin-user fraiseql \
  --admin-password <password> \
  --sku-name B_Gen5_1

# 2. Get connection string
az postgres server show \
  --resource-group fraiseql \
  --name fraiseql-users \
  --query 'fullyQualifiedDomainName'

# 3. Create container registry
az acr create --resource-group fraiseql --name fraiseql

# 4. Build and push
az acr build \
  --registry fraiseql \
  --image fraiseql-users:latest \
  users-service/

# 5. Deploy container
az container create \
  --resource-group fraiseql \
  --name fraiseql-users \
  --image fraiseql.azurecr.io/fraiseql-users:latest \
  --environment-variables DATABASE_URL=postgresql://...
```

**Expected Performance:**
- Single query: <5ms
- Batch 100: ~10ms
- Cross-region: +15-25ms latency

---

## Multi-Cloud Deployment

### Architecture

```
     Federation Gateway (Apollo Router / Kong)
            |
    +-------+-------+-------+
    |       |       |       |
   AWS     GCP    Azure   On-Prem
   |       |       |       |
 Users   Orders  Products Inventory
  DB      DB      DB      DB
```

### Deployment Steps

#### 1. Prepare Infrastructure

```bash
# AWS: Users Service
aws rds create-db-instance --db-instance-identifier users --region us-east-1
aws ecr create-repository --repository-name fraiseql-users

# GCP: Orders Service
gcloud sql instances create orders --region europe-west1
gcloud container registries create --location=eu gcr.io/PROJECT/fraiseql-orders

# Azure: Products Service
az postgres server create --resource-group prod --name products --location westeurope
az acr create --resource-group prod --name fraiseql
```

#### 2. Configure Federation

**federation.toml (shared across all subgraphs):**

```toml
[federation]
enabled = true
version = "v2"

# AWS: Users Service
[[federation.subgraphs]]
name = "User"
strategy = "local"

# GCP: Orders Service (HTTP for cross-cloud)
[[federation.subgraphs]]
name = "Order"
strategy = "http"
url = "https://orders.example.com/graphql"

# Azure: Products Service (HTTP for cross-cloud)
[[federation.subgraphs]]
name = "Product"
strategy = "http"
url = "https://products.example.com/graphql"

[federation.http]
timeout_ms = 5000
max_retries = 3
retry_delay_ms = 100
```

#### 3. Deploy Services

```bash
# Deploy Users (AWS)
./deploy.sh aws us-east-1 users-service

# Deploy Orders (GCP)
./deploy.sh gcp europe-west1 orders-service

# Deploy Products (Azure)
./deploy.sh azure westeurope products-service

# Deploy Gateway (Optional - use Apollo Router)
./deploy-gateway.sh
```

#### 4. Verify Federation

```bash
# Check Users service is reachable
curl https://users.example.com/graphql -d '{"query": "{users{id}}"}'

# Check Orders service is reachable
curl https://orders.example.com/graphql -d '{"query": "{orders{id}}"}'

# Check Products service is reachable
curl https://products.example.com/graphql -d '{"query": "{products{id}}"}'

# Test federated query
curl https://gateway.example.com/graphql \
  -d '{"query": "{user(id:\"1\"){id orders{id products{id}}}}"}'
```

---

### Expected Multi-Cloud Performance

| Scenario | Latency | Notes |
|----------|---------|-------|
| Local query (same region) | <5ms | Direct DB access |
| Cross-cloud query | 20-50ms | 2x cross-cloud hops |
| 3-tier hierarchy | 50-100ms | 3x cross-cloud hops |
| Batch 100 local | ~10ms | Batched local DB |
| Batch 100 cross-cloud | ~50-100ms | Batched HTTP |

---

## Performance Optimization

### 1. Database Indexing

**Critical indexes:**

```sql
-- Key field indexes
CREATE INDEX idx_id ON users(id);
CREATE INDEX idx_org_id_user_id ON users(organization_id, id);

-- Foreign key indexes
CREATE INDEX idx_user_id ON orders(user_id);
CREATE INDEX idx_product_id ON orders(product_id);

-- Query optimization
CREATE INDEX idx_status ON orders(status) WHERE status != 'completed';
```

**Impact:** 10-50x query speedup for federation

---

### 2. Connection Pooling

**FraiseQL configuration:**

```toml
[database]
pool_size = 20           # Connections per pool
max_idle_time = 300      # Seconds
connection_timeout = 5   # Seconds
```

**Impact:** 20-30% reduction in query latency

---

### 3. Query Caching

```toml
[cache]
enabled = true
ttl_seconds = 300        # 5-minute cache
max_size_mb = 256        # Cache size limit

# Cache federation queries
[[cache.patterns]]
query = "_entities"
ttl_seconds = 60         # Shorter TTL for entities
```

**Impact:** 50-90% reduction for repeated queries

---

### 4. Result Projection

Always query only needed fields:

```graphql
# ❌ Inefficient: Queries all fields
query {
  user(id: "123") {
    id
    name
    email
    phone
    address
    orders { id total amount }
  }
}

# ✅ Efficient: Only needed fields
query {
  user(id: "123") {
    id
    name
    orders { id total }
  }
}
```

**Impact:** 20-40% reduction in payload and latency

---

## Monitoring & Observability

### 1. Prometheus Metrics

```yaml
# Add to FraiseQL config
[observability]
prometheus_enabled = true
prometheus_port = 9090

# Metrics exposed:
# - fraiseql_federation_resolution_ms: Resolution latency
# - fraiseql_federation_entities_resolved: Entities resolved
# - fraiseql_federation_cache_hits: Cache hit rate
# - fraiseql_database_queries_total: Total queries
```

---

### 2. Grafana Dashboard

Key metrics to monitor:

```

- Federation Latency (P50, P95, P99)
- Entity Resolution Success Rate
- Cache Hit Rate
- Database Connection Pool Utilization
- HTTP Federation Error Rate
- Cross-Cloud Latency
```

---

### 3. Logging

**Structured logging for debugging:**

```bash
# Enable debug logging
export RUST_LOG=fraiseql_core::federation=debug

# Log entries include:
# - Entity resolution strategy selected
# - Batching information
# - Remote subgraph calls
# - Cache hits/misses
# - Latency breakdown
```

---

### 4. Alerting

**Critical alerts:**

```yaml
- name: FederationHighLatency
  condition: p99_latency > 500ms
  action: page_on_call

- name: FederationErrors
  condition: error_rate > 1%
  action: page_on_call

- name: CacheHitRateLow
  condition: cache_hit_rate < 50%
  action: investigate

- name: DatabaseConnPoolExhausted
  condition: idle_connections == 0
  action: scale_up
```

---

## Troubleshooting

### Issue: Slow Cross-Cloud Queries

**Symptoms:** Queries >200ms latency

**Solutions:**
1. Use DirectDB strategy if both are FraiseQL
2. Enable query result caching
3. Optimize field selection (only request needed fields)
4. Check network latency between clouds

```bash
# Test network latency
ping orders-service.example.com
traceroute orders-service.example.com
```

---

### Issue: Federation Timeouts

**Symptoms:** `Error: Request timeout after 5000ms`

**Solutions:**
1. Increase timeout in config:
   ```toml
   [federation.http]
   timeout_ms = 10000  # Increase from 5000
   ```

2. Check remote service health:
   ```bash
   curl -v https://orders.example.com/health
   ```

3. Check network connectivity:
   ```bash
   curl -w "@curl-format.txt" -o /dev/null -s https://orders.example.com/graphql
   ```

---

### Issue: High Error Rate

**Symptoms:** >1% of federation queries fail

**Solutions:**
1. Check subgraph availability
   ```bash
   curl https://orders.example.com/_service
   ```

2. Review logs for specific errors
   ```bash
   kubectl logs -l app=orders-service --tail=100
   ```

3. Enable retry logic (automatic in FraiseQL):
   ```toml
   [federation.http]
   max_retries = 5
   retry_delay_ms = 100
   ```

---

## Production Checklist

- [ ] All subgraphs deployed and healthy
- [ ] Database indexes created on key fields
- [ ] Connection pooling configured
- [ ] Query result caching enabled
- [ ] Monitoring and alerting configured
- [ ] Logging and debugging enabled
- [ ] Load testing completed (>100 qps)
- [ ] Backup and recovery tested
- [ ] Network latency acceptable (<100ms)
- [ ] Error rate <0.1%
- [ ] P99 latency <500ms

---

## Next Steps

1. **Start Small:** Single cloud, single subgraph
2. **Add Services:** Second subgraph (federation)
3. **Optimize:** Implement caching and monitoring
4. **Scale:** Add more subgraphs/clouds
5. **Monitor:** Track performance and user experience

