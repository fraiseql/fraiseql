# Role-Specific Quick Start Guides

**Status:** ✅ Production Ready
**Reading Time:** 20-30 minutes per role
**Last Updated:** 2026-02-05

Tailored quick-start guides for different roles and experience levels. Select your role:

---

## 👨‍💻 1. Backend Developer (5-Minute Quick Start)

**Goal:** Get a simple GraphQL API running locally

### Prerequisites

```bash
# Install FraiseQL
cargo install fraiseql-cli

# Install SDK for your language
# Python:
pip install fraiseql

# TypeScript:
npm install @fraiseql/client

# Go:
go get github.com/fraiseql/fraiseql-go/v2
```

### Create Your First Schema

```python
# schema.py
from fraiseql import type, field

@type
class User:
    id: str
    name: str
    email: str
    created_at: str

@type
class Post:
    id: str
    user_id: str
    title: str
    content: str
    user: User
```

### Configure Database

```toml
# fraiseql.toml
[database]
url = "postgresql://localhost/myapp"
pool_size = 10
```

### Compile and Run

```bash
# Compile schema
fraiseql compile schema.py --config fraiseql.toml

# Start server
fraiseql serve

# Test GraphQL endpoint
curl -X POST http://localhost:5000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{
      users {
        id
        name
        email
      }
    }"
  }'
```

### Query from Your App

```python
from fraiseql import AsyncClient
import asyncio

async def main():
    async with AsyncClient(url="http://localhost:5000") as client:
        result = await client.query("""
            query {
              users {
                id
                name
                email
              }
            }
        """)
        for user in result["users"]:
            print(f"{user['name']} ({user['email']})")

asyncio.run(main())
```

### Next Steps

- [ ] Add mutations (CREATE, UPDATE, DELETE)
- [ ] Add authentication (OAuth2)
- [ ] Add filtering and pagination
- → Read: [Common Patterns](./PATTERNS.md)

---

## 🏗️ 2. Architect / Schema Designer (15-Minute Walkthrough)

**Goal:** Design a scalable, federated schema

### Step 1: Map Your Domain

```
Users Service (Service 1)
├─ Table: users
├─ Fields: id, name, email, created_at
├─ Primary Key: id
└─ Relationships: 1:M → Posts

Orders Service (Service 2)
├─ Table: orders
├─ Fields: id, user_id, total, status, created_at
├─ Primary Key: id
├─ Foreign Key: user_id → users.id (federation)
└─ Relationships: 1:M → Items
```

### Step 2: Define Federated Types

```python
# services/users/schema.py
@type
@key("id")
class User:
    """User owned by Users Service."""
    id: str
    name: str
    email: str
    created_at: str

# services/orders/schema.py
@type
@extends
@key("id")
class User:
    """User extended in Orders Service."""
    id: str = field(external())
    email: str = field(external())
    orders: List[Order]  # Users can view their orders

@type
@key("id")
class Order:
    """Order owned by Orders Service."""
    id: str
    user_id: str
    total: Decimal
    status: OrderStatus
    created_at: str
    user: User  # Reference back to user
```

### Step 3: Add Authorization

```python
@type
@extends
@key("id")
class User:
    where: Where = fraiseql.where(
        fk_org=fraiseql.context.org_id  # Multi-tenancy
    )

    id: str = field(external())
    email: str = field(
        external(),
        authorize={Roles.SELF, Roles.ADMIN}
    )
    orders: List[Order]
```

### Step 4: Optimize with Views

```python
@type
class UserStats:
    """Materialized daily for performance."""
    id: str
    order_count: int
    total_spent: Decimal
    avg_order_value: Decimal
    updated_at: str
```

### Step 5: Configure Deployment

```toml
# fraiseql.toml
[fraiseql.federation]
enabled = true
strategy = "direct-database"  # Direct DB is fast

[[fraiseql.subgraphs]]
name = "Orders"
strategy = "direct-database"
database_url = "${ORDERS_DATABASE_URL}"

[[fraiseql.subgraphs]]
name = "Users"
strategy = "http"
url = "http://users-service:5000/graphql"
```

### Validation Checklist

- [ ] All types have `@key` directives
- [ ] All relationships documented
- [ ] Authorization policies defined
- [ ] High-cardinality fields have materialized views
- [ ] Federation strategy chosen per subgraph

### Next Steps

- [ ] Design database schema to support views
- [ ] Create materialization jobs
- → Read: [Schema Design Best Practices](./schema-design-best-practices.md)

---

## 🛠️ 3. DevOps / SRE (20-Minute Setup)

**Goal:** Deploy FraiseQL to production with monitoring

### Step 1: Containerization

```dockerfile
# Dockerfile
FROM rust:latest as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/fraiseql /usr/local/bin/
COPY fraiseql.toml /etc/fraiseql/
EXPOSE 5000
ENTRYPOINT ["fraiseql", "serve"]
```

### Step 2: Configuration Management

```toml
# fraiseql.toml
[server]
port = 5000
tls_enabled = true
tls_cert_path = "/etc/certs/server.crt"
tls_key_path = "/etc/certs/server.key"

[database]
url = "${FRAISEQL_DATABASE_URL}"  # Environment variable
pool_size = 20
connection_timeout_seconds = 10

[security]
rate_limit_enabled = true
auth_required = true

[monitoring]
metrics_enabled = true
tracing_enabled = true
```

### Step 3: Kubernetes Deployment

```yaml
# k8s/fraiseql-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql
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
        image: my-registry/fraiseql:v2.0.0
        ports:
        - containerPort: 5000
        env:
        - name: FRAISEQL_DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: database-url
        resources:
          requests:
            memory: "512Mi"
            cpu: "250m"
          limits:
            memory: "1Gi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 5000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 5000
          initialDelaySeconds: 5
          periodSeconds: 5
```

### Step 4: Monitoring & Logging

```yaml
# k8s/prometheus-config.yaml
global:
  scrape_interval: 15s

scrape_configs:
- job_name: 'fraiseql'
  static_configs:
  - targets: ['localhost:9090']

# Alert rules
groups:
- name: fraiseql
  rules:
  - alert: HighErrorRate
    expr: fraiseql_errors_total{job="fraiseql"} > 100
    for: 5m
  - alert: SlowQueries
    expr: fraiseql_query_latency_p95 > 1000  # ms
    for: 5m
```

### Step 5: Secrets Management

```bash
# Create secrets
kubectl create secret generic fraiseql-secrets \
  --from-literal=database-url="postgresql://user:pass@db:5432/fraiseql" \
  --from-literal=jwt-secret="your-jwt-secret-here"

# Verify
kubectl get secrets
```

### Deployment Checklist

- [ ] Docker image building correctly
- [ ] Environment variables set
- [ ] Health checks passing
- [ ] Monitoring metrics flowing
- [ ] Alerts configured
- [ ] Log aggregation working

### Next Steps

- [ ] Set up auto-scaling
- [ ] Configure backup strategy
- → Read: [Production Deployment](./production-deployment.md)

---

## 📊 4. Data Analyst / BI Developer (15-Minute Setup)

**Goal:** Export data to BI tool using Arrow Flight

### Step 1: Define Analytical Views

```python
# schema.py
@type
class SalesAnalytics:
    """Daily sales data in Arrow format."""
    date: Date
    region: str
    product_id: str
    units_sold: int
    revenue: Decimal
    cost: Decimal
    profit: Decimal

@type
class CustomerSegmentation:
    """Customer behavior analytics."""
    customer_id: str
    segment: str  # "high-value", "churn-risk", etc.
    ltv: Decimal  # Lifetime value
    days_active: int
    purchase_frequency: float
```

### Step 2: Enable Arrow Flight

```toml
# fraiseql.toml
[arrow_flight]
enabled = true
port = 30000
tls_enabled = false  # Enable in production!
```

### Step 3: Connect BI Tool

**DuckDB (for SQL exploration):**

```python
import duckdb

conn = duckdb.connect()
conn.register_arrow_object("sales", "grpc://localhost:30000/SalesAnalytics")

# Query directly on Arrow data!
result = conn.execute("""
    SELECT date, SUM(revenue) as daily_revenue
    FROM sales
    GROUP BY date
    ORDER BY date DESC
""").fetchall()
```

**Tableau (native Arrow Flight connector):**

1. Data Source → Arrow Flight
2. Server: `localhost`
3. Port: `30000`
4. Dataset: `SalesAnalytics`
5. Create visualization!

**Python Analytics:**

```python
import pandas as pd
import pyarrow.flight as flight

client = flight.connect("grpc://localhost:30000")
reader = client.do_get(flight.Ticket(b"SalesAnalytics"))
df = reader.read_pandas()

# Use pandas normally
print(df.groupby('region')['revenue'].sum())

# Or convert to Polars for performance
import polars as pl
pl_df = pl.from_arrow(reader.read_all())
```

### Step 4: Schedule Exports

```bash
# Daily export to data warehouse
0 2 * * * /scripts/export-analytics.sh

# Script:
#!/bin/bash
python3 << 'EOF'
import pandas as pd
import pyarrow.flight as flight
from sqlalchemy import create_engine

client = flight.connect("grpc://fraiseql-server:30000")
reader = client.do_get(flight.Ticket(b"SalesAnalytics"))
df = reader.read_pandas()

# Write to warehouse
engine = create_engine("postgresql://warehouse/analytics")
df.to_sql('sales_analytics', engine, if_exists='append', index=False)
EOF
```

### Next Steps

- [ ] Connect Tableau/PowerBI/Looker
- [ ] Set up automated exports
- → Read: [Arrow Flight Quick Start](./arrow-flight-quick-start.md)

---

## 🚀 5. Startup Founder (20-Minute End-to-End)

**Goal:** Launch a complete multi-tenant SaaS backend

### Step 1: Design SaaS Schema

```python
# schema.py
@type
class Organization:
    """Customer organization."""
    id: str
    name: str
    plan: PlanTier  # "free", "pro", "enterprise"
    created_at: str
    users: List[User]
    projects: List[Project]

@type
class User:
    """User within organization (multi-tenant)."""
    where: Where = fraiseql.where(
        fk_org=fraiseql.context.org_id  # Automatic tenant isolation
    )

    id: str
    org_id: str
    name: str
    email: str = field(authorize={Roles.SELF, Roles.ADMIN})
    role: UserRole  # "owner", "admin", "member"
    created_at: str

@type
class Project:
    """User's project within organization."""
    where: Where = fraiseql.where(
        fk_org=fraiseql.context.org_id
    )

    id: str
    org_id: str
    name: str
    description: str
    data: JSON  # Flexible project data
    owner_id: str
    created_at: str
```

### Step 2: Configure Authentication

```toml
[authentication.oauth]
enabled = true
provider = "google"  # Or Auth0, GitHub, etc.
client_id = "${OAUTH_CLIENT_ID}"
client_secret = "${OAUTH_CLIENT_SECRET}"
redirect_url = "https://app.example.com/auth/callback"

[authentication.jwt]
algorithm = "RS256"
issuer = "https://auth.example.com"
audience = "https://api.example.com"
```

### Step 3: Add Rate Limiting

```toml
[rate_limiting]
enabled = true

[rate_limiting.auth]
max_requests = 10
window_seconds = 60  # 10 requests/minute

[rate_limiting.graphql]
max_requests = 100
window_seconds = 60  # 100 requests/minute per user
```

### Step 4: Deploy to Production

```bash
# On your server:
# 1. Clone repo
git clone https://github.com/yourcompany/fraiseql-backend
cd fraiseql-backend

# 2. Configure environment
export FRAISEQL_DATABASE_URL="postgresql://..."
export OAUTH_CLIENT_ID="..."
export OAUTH_CLIENT_SECRET="..."

# 3. Build Docker image
docker build -t myapp-backend:latest .

# 4. Push to registry
docker push myapp-backend:latest

# 5. Deploy with docker-compose
docker-compose up -d

# 6. Test API
curl https://api.example.com/health
```

### Step 5: Set Up Webhooks

```python
@fraiseql.observer
class SendWelcomeEmail:
    trigger = Event.CREATE
    entity = "User"

    actions = [
        Email(
            to=event.data.get("email"),
            template="welcome_to_saas",
            variables={"org_name": event.context.org_name}
        )
    ]
```

### Launch Checklist

- [ ] Database configured
- [ ] Authentication working
- [ ] Rate limiting tested
- [ ] Webhooks sending
- [ ] Monitoring alerts set
- [ ] HTTPS enabled
- [ ] Health check passing

### Next Steps

- [ ] Add Stripe billing integration
- [ ] Set up customer support chat
- [ ] Add feature flags for A/B testing
- → Read: [Production Deployment](./production-deployment.md)

---

## Quick Role Reference

| Role | Time | Focus | Next |
|---|---|---|---|
| Backend Dev | 5 min | Get running locally | [PATTERNS](./PATTERNS.md) |
| Architect | 15 min | Design schema | [Schema Design](./schema-design-best-practices.md) |
| DevOps/SRE | 20 min | Deploy to production | [Production Deploy](./production-deployment.md) |
| Data Analyst | 15 min | Export to BI tool | [Arrow Flight](./arrow-flight-quick-start.md) |
| Startup | 20 min | Launch SaaS | [All guides] |

---

## See Also

**All Quick Starts:**

- **[Federation Quick Start](./federation-quick-start.md)** — Multi-database setup
- **[Authorization Quick Start](./authorization-quick-start.md)** — Field-level access control
- **[Arrow Flight Quick Start](./arrow-flight-quick-start.md)** — Analytics export

**Complete Guides:**

- **[Common Patterns](./PATTERNS.md)** — Real-world solutions
- **[Schema Design](./schema-design-best-practices.md)** — Design patterns
- **[Production Deployment](./production-deployment.md)** — Deployment procedures

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
