# Admin Panel Example

Production-ready admin panel example demonstrating how to build internal tools with FraiseQL for customer support, operations management, and sales dashboards.

## What This Example Demonstrates

This is a **complete admin panel application** showing:
- ✅ Customer support dashboard with search and account management
- ✅ Operations dashboard for order management and fulfillment
- ✅ Sales metrics and pipeline management
- ✅ Role-based access control with audit logging
- ✅ Read-only views for safe production data access
- ✅ Real-time metrics and reporting
- ✅ FastAPI integration with GraphQL Playground

## Use Cases

### Customer Support Dashboard
**Problem:** Support teams need quick access to customer information without direct database access.

**Solution:** FraiseQL provides read-only views with safe search capabilities:
```graphql
query SearchCustomers($query: String!, $status: String) {
  customerSearch(query: $query, status: $status) {
    id
    email
    name
    subscriptionStatus
    totalSpent
    supportTickets {
      id
      subject
      status
      createdAt
    }
  }
}
```

### Operations Dashboard
**Problem:** Operations teams need visibility into orders, inventory, and fulfillment status.

**Solution:** Real-time PostgreSQL views provide live production data:
```graphql
query OperationsMetrics {
  operationsMetrics {
    pendingOrders
    averageFulfillmentTime
    inventoryLowStock
    todayRevenue
  }

  recentOrders(limit: 50) {
    id
    customer { name email }
    items { product quantity }
    status
    createdAt
  }
}
```

### Sales Dashboard
**Problem:** Sales teams need real-time pipeline visibility and deal management.

**Solution:** Live metrics with mutation support for deal updates:
```graphql
query SalesDashboard {
  salesMetrics {
    repId
    repName
    currentMonthRevenue
    quotaAttainment
    dealsInPipeline
    averageDealSize
  }
}

mutation UpdateDeal($input: DealUpdateInput!) {
  updateDealStage(input: $input) {
    dealId
    stage
    amount
    notes
  }
}
```

## Architecture

### Read-Only Views for Safety

All queries use PostgreSQL views to provide read-only access to production data:

```sql
-- Customer support view (safe, no sensitive data)
CREATE VIEW customer_admin_view AS
SELECT
    u.id,
    u.email,
    u.name,
    u.created_at,
    s.status as subscription_status,
    COALESCE(SUM(o.total), 0) as total_spent,
    COUNT(t.id) as ticket_count
FROM users u
LEFT JOIN subscriptions s ON s.user_id = u.id
LEFT JOIN orders o ON o.user_id = u.id
LEFT JOIN support_tickets t ON t.user_id = u.id
GROUP BY u.id, u.email, u.name, u.created_at, s.status;
```

### Role-Based Access Control

Different admin roles have different permissions:

```python
from fraiseql.auth import requires_role

@fraiseql.query
@requires_role("customer_support")
async def customer_search(info, query: str) -> list[CustomerInfo]:
    """Customer support can search customers"""
    return await info.context.repo.find("customer_admin_view", ...)

@fraiseql.mutation
@requires_role("admin")
async def update_customer_status(
    info,
    customer_id: UUID,
    new_status: str
) -> CustomerInfo:
    """Only admins can change customer status"""
    # Audit log automatically created
    return await info.context.repo.update(...)
```

### Automatic Audit Logging

All admin actions are automatically logged:

```sql
CREATE TABLE admin_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    admin_user_id UUID NOT NULL,
    action VARCHAR(100) NOT NULL,
    target_type VARCHAR(50),
    target_id UUID,
    details JSONB,
    ip_address INET,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Example audit entry
{
    "admin_user_id": "123e4567-e89b-12d3-a456-426614174000",
    "action": "update_customer_status",
    "target_type": "customer",
    "target_id": "789e4567-e89b-12d3-a456-426614174999",
    "details": {
        "old_status": "active",
        "new_status": "suspended",
        "reason": "Payment failed after 3 attempts"
    },
    "created_at": "2025-10-08T15:30:00Z"
}
```

## Setup

### 1. Install Dependencies

```bash
cd examples/admin-panel
pip install -r requirements.txt
```

### 2. Setup Database

```bash
# Create database
createdb admin_panel_demo

# Run schema
psql admin_panel_demo < schema.sql

# Optional: Load sample data
psql admin_panel_demo < seed_data.sql
```

### 3. Configure Environment

```bash
cp .env.example .env
# Edit .env with your database URL and admin credentials
```

### 4. Run the Application

```bash
python main.py
```

The admin panel will be available at:
- **GraphQL API:** http://localhost:8000/graphql
- **GraphQL Playground:** http://localhost:8000/graphql
- **API Documentation:** http://localhost:8000/docs

## Features

### Customer Support Tools

#### Search Customers
```graphql
query SearchCustomers {
  customerSearch(query: "john@example.com") {
    id
    email
    name
    createdAt
    subscriptionStatus
    totalSpent
    supportTickets {
      id
      subject
      status
      priority
      createdAt
    }
    recentOrders(limit: 5) {
      id
      total
      status
      createdAt
    }
  }
}
```

#### View Support Tickets
```graphql
query OpenTickets {
  supportTickets(status: "open", limit: 50) {
    id
    customer { name email }
    subject
    priority
    assignedTo { name }
    createdAt
    lastUpdated
  }
}
```

#### Update Customer Account
```graphql
mutation UpdateCustomer($id: UUID!, $input: CustomerUpdateInput!) {
  updateCustomerStatus(
    customerId: $id
    newStatus: "suspended"
    reason: "Fraud investigation"
  ) {
    id
    subscriptionStatus
    updatedAt
  }
}
```

### Operations Dashboard

#### Real-Time Metrics
```graphql
query OperationsDashboard {
  operationsMetrics {
    # Order metrics
    pendingOrders
    processingOrders
    shippedToday
    averageFulfillmentTime

    # Inventory metrics
    lowStockItems
    outOfStockItems

    # Revenue metrics
    todayRevenue
    monthRevenue

    # Performance metrics
    orderAccuracy
    onTimeDeliveryRate
  }
}
```

#### Order Management
```graphql
query OrdersNeedingAttention {
  orders(
    where: {
      status: { in: ["pending", "processing"] }
      createdAt: { gte: "-7 days" }
    }
    orderBy: { createdAt: DESC }
    limit: 100
  ) {
    id
    orderNumber
    customer { name email }
    items {
      product { name sku }
      quantity
      price
    }
    total
    status
    createdAt
    estimatedShipDate
  }
}
```

#### Update Order Status
```graphql
mutation UpdateOrderStatus($orderId: UUID!, $status: String!, $notes: String) {
  updateOrderStatus(
    orderId: $orderId
    newStatus: $status
    notes: $notes
  ) {
    id
    status
    updatedAt
    statusHistory {
      status
      changedBy { name }
      notes
      timestamp
    }
  }
}
```

### Sales Dashboard

#### Sales Metrics
```graphql
query SalesTeamMetrics {
  salesMetrics {
    teamMetrics {
      totalRevenue
      averageDealSize
      winRate
      averageSalesCycle
    }

    repMetrics {
      repId
      repName
      currentMonthRevenue
      quotaAttainment
      dealsInPipeline
      dealsWonThisMonth
      averageDealSize
    }

    pipelineByStage {
      stage
      dealCount
      totalValue
      averageAge
    }
  }
}
```

#### Deal Management
```graphql
query MyPipeline($repId: UUID!) {
  deals(
    where: {
      assignedTo: $repId
      stage: { notIn: ["closed_won", "closed_lost"] }
    }
    orderBy: { expectedCloseDate: ASC }
  ) {
    id
    company { name }
    contact { name email }
    stage
    amount
    probability
    expectedCloseDate
    lastActivity
    notes
  }
}
```

#### Update Deal
```graphql
mutation MoveDealStage($dealId: UUID!, $newStage: String!, $notes: String) {
  updateDealStage(
    dealId: $dealId
    stage: $newStage
    notes: $notes
  ) {
    id
    stage
    amount
    probability
    updatedAt

    # Trigger notifications/webhooks
    notifications {
      type
      recipient
      message
    }
  }
}
```

## Security Features

### 1. Role-Based Access Control

```python
# Different roles for different admin functions
ADMIN_ROLES = {
    "super_admin": ["*"],  # Full access
    "customer_support": [
        "customer:read",
        "customer:update_basic",
        "ticket:read",
        "ticket:update",
    ],
    "operations": [
        "order:read",
        "order:update_status",
        "inventory:read",
        "inventory:update",
    ],
    "sales": [
        "deal:read",
        "deal:update",
        "customer:read",
        "metrics:sales",
    ],
    "readonly": [
        "customer:read",
        "order:read",
        "metrics:read",
    ],
}
```

### 2. Audit Trail for All Actions

Every mutation automatically logs:
- Who performed the action (admin user)
- What action was performed
- What entity was modified
- Before/after values
- Timestamp and IP address

```python
@fraiseql.mutation
@requires_role("admin")
async def update_customer_status(
    info,
    customer_id: UUID,
    new_status: str,
    reason: str
) -> CustomerInfo:
    """Update customer subscription status with automatic audit logging."""

    # Get current status
    customer = await info.context.repo.find_one("customers", customer_id)

    # Log the action
    await info.context.repo.create("admin_audit_log", {
        "admin_user_id": info.context.user["id"],
        "action": "update_customer_status",
        "target_type": "customer",
        "target_id": customer_id,
        "details": {
            "old_status": customer["subscription_status"],
            "new_status": new_status,
            "reason": reason
        },
        "ip_address": info.context.request.client.host
    })

    # Perform update
    return await info.context.repo.update(
        "customers",
        customer_id,
        {"subscription_status": new_status}
    )
```

### 3. Read-Only Views by Default

All queries use database views that:
- Filter out sensitive data (passwords, tokens, etc.)
- Aggregate related data for efficiency
- Provide pre-computed metrics
- Cannot modify underlying tables

```sql
-- Safe view excludes sensitive fields
CREATE VIEW customer_admin_view AS
SELECT
    id,
    email,
    name,
    created_at,
    subscription_status,
    -- NO password_hash
    -- NO reset_tokens
    -- NO api_keys
    total_spent,
    ticket_count
FROM users u
LEFT JOIN aggregated_metrics m ON m.user_id = u.id;
```

## Performance Considerations

### Indexed Views for Fast Queries

```sql
-- Materialized view for dashboard metrics (refreshed every 5 minutes)
CREATE MATERIALIZED VIEW operations_metrics_mv AS
SELECT
    COUNT(*) FILTER (WHERE status = 'pending') as pending_orders,
    COUNT(*) FILTER (WHERE status = 'processing') as processing_orders,
    AVG(fulfilled_at - created_at) FILTER (WHERE fulfilled_at IS NOT NULL)
        as average_fulfillment_time,
    SUM(total) FILTER (WHERE created_at >= CURRENT_DATE) as today_revenue,
    -- ... more metrics
FROM orders
WHERE created_at >= CURRENT_DATE - INTERVAL '30 days';

-- Refresh on schedule
CREATE INDEX ON operations_metrics_mv (last_refreshed);
```

### Pagination for Large Datasets

```graphql
query PaginatedOrders($cursor: String, $limit: Int = 50) {
  orders(after: $cursor, limit: $limit) {
    edges {
      node {
        id
        orderNumber
        customer { name }
        total
        status
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```

## Integration with Frontend

### React Admin Integration

```typescript
import { Admin, Resource, ListGuesser } from 'react-admin';
import buildGraphQLProvider from 'ra-data-graphql-simple';

// FraiseQL endpoint
const dataProvider = buildGraphQLProvider({
  clientOptions: { uri: 'http://localhost:8000/graphql' }
});

const App = () => (
  <Admin dataProvider={dataProvider}>
    <Resource name="customers" list={CustomerList} />
    <Resource name="orders" list={OrderList} />
    <Resource name="tickets" list={TicketList} />
  </Admin>
);
```

### Retool Integration

1. Add GraphQL datasource: `http://localhost:8000/graphql`
2. Create queries using GraphQL editor
3. Bind to Retool components (tables, forms, charts)
4. Deploy to team

### Custom Frontend (Next.js + Apollo)

```typescript
import { ApolloClient, InMemoryCache, HttpLink } from '@apollo/client';

const client = new ApolloClient({
  link: new HttpLink({
    uri: 'http://localhost:8000/graphql',
    headers: {
      authorization: `Bearer ${adminToken}`,
    }
  }),
  cache: new InMemoryCache()
});

// Use in your admin dashboard
const { data } = useQuery(OPERATIONS_METRICS);
```

## Monitoring & Observability

### Track Admin Actions

```python
from prometheus_client import Counter, Histogram

admin_actions = Counter(
    'admin_actions_total',
    'Total admin actions performed',
    ['action_type', 'admin_role']
)

admin_action_duration = Histogram(
    'admin_action_duration_seconds',
    'Admin action duration',
    ['action_type']
)

@fraiseql.mutation
@requires_role("admin")
async def update_customer_status(info, customer_id: UUID, new_status: str):
    with admin_action_duration.labels('update_customer_status').time():
        result = await perform_update(customer_id, new_status)
        admin_actions.labels(
            action_type='update_customer_status',
            admin_role=info.context.user['role']
        ).inc()
        return result
```

### Dashboard Metrics

Monitor admin panel usage:
- Most common queries
- Slowest operations
- Most active admin users
- Error rates by action type

## Troubleshooting

### Slow Customer Search

**Problem:** Customer search taking >2 seconds with 100k+ users

**Solution:** Add full-text search index
```sql
-- Add GIN index for text search
ALTER TABLE users ADD COLUMN search_vector tsvector
    GENERATED ALWAYS AS (
        to_tsvector('english', coalesce(name, '') || ' ' || coalesce(email, ''))
    ) STORED;

CREATE INDEX idx_users_search ON users USING GIN(search_vector);

-- Use in query
SELECT * FROM users
WHERE search_vector @@ to_tsquery('english', 'john');
```

### Audit Logs Growing Too Large

**Problem:** admin_audit_log table using too much disk space

**Solution:** Partition by month and archive old data
```sql
-- Create partitioned table
CREATE TABLE admin_audit_log_partitioned (
    id UUID DEFAULT gen_random_uuid(),
    admin_user_id UUID NOT NULL,
    action VARCHAR(100) NOT NULL,
    details JSONB,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
) PARTITION BY RANGE (created_at);

-- Create monthly partitions
CREATE TABLE admin_audit_log_2025_10
    PARTITION OF admin_audit_log_partitioned
    FOR VALUES FROM ('2025-10-01') TO ('2025-11-01');

-- Archive to S3 after 90 days
```

## Related Examples

- [`../fastapi/`](../fastapi/) - Complete FastAPI integration
- [`../enterprise_patterns/cqrs/`](../enterprise_patterns/cqrs/) - CQRS pattern for mutations
- [`../saas-starter/`](../saas-starter/) - Multi-tenant SaaS application

## Production Deployment

### Environment Variables

```bash
DATABASE_URL=postgresql://user:pass@db:5432/admin_panel
ADMIN_SECRET_KEY=your-secret-key-here
CORS_ORIGINS=https://admin.yourcompany.com
LOG_LEVEL=INFO
SENTRY_DSN=https://...
```

### Docker Deployment

```dockerfile
FROM python:3.13-slim

WORKDIR /app
COPY requirements.txt .
RUN pip install -r requirements.txt

COPY . .

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000"]
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: admin-panel
spec:
  replicas: 3
  selector:
    matchLabels:
      app: admin-panel
  template:
    spec:
      containers:
      - name: admin-panel
        image: yourcompany/admin-panel:latest
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: admin-panel-secrets
              key: database-url
```

## Next Steps

1. **Customize for your domain** - Adapt models and queries to your business
2. **Add more dashboards** - Finance, marketing, product analytics
3. **Integrate with tools** - Slack notifications, PagerDuty alerts
4. **Build frontend** - React Admin, Retool, or custom Next.js app
5. **Add more roles** - Fine-grained permissions for your team structure

---

**This example demonstrates a production-ready admin panel with FraiseQL. Safe database access, comprehensive audit logging, and role-based permissions out of the box!** ✨
