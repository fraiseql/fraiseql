# Fraisier as a GraphQL API

## Vision

**Fraisier will eventually expose its entire functionality as a GraphQL API** using FraiseQL's compiled execution engine.

This means:

- Fraisier deployments status is **queryable via GraphQL**
- Deployments can be **triggered via mutations**
- Status changes are **available via subscriptions**
- The API is **self-documenting and type-safe**
- **No custom API code needed** - entirely generated from schema

---

## Why GraphQL?

### Traditional REST API

```
GET  /api/fraises                         (list all)
GET  /api/fraises/:id                     (get one)
GET  /api/fraises/:id/environments        (get environments)
GET  /api/deployments                     (list all)
GET  /api/deployments?fraiseId=X          (filter)
POST /api/deployments (body: fraise, env) (trigger)
```

**Problems:**

- Multiple endpoints to fetch related data (N+1 queries)
- Over-fetching (getting fields you don't need)
- Under-fetching (need multiple requests)
- Hard to evolve without breaking clients

### GraphQL API

```graphql
query {
  fraise(id: "my_api") {
    id
    name
    deploymentHistory(limit: 5) {
      id
      status
    }
  }
}
```

**Benefits:**
✅ Single endpoint
✅ Client specifies exactly what data needed
✅ No over/under-fetching
✅ Evolve schema without breaking clients
✅ Built-in documentation

---

## Complete GraphQL Schema

### Core Types

```graphql
"""A deployable service"""
type Fraise {
  id: String!
  name: String!
  type: FraiseType!
  description: String
  environments: [Environment!]!
  currentStatus: FraiseStatus!
  deploymentHistory(limit: Int = 50): [Deployment!]!
}

"""Fraise type categorization"""
enum FraiseType {
  API
  ETL
  SCHEDULED
  BACKUP
}

"""Environment configuration"""
type Environment {
  name: String!
  branch: String!
  appPath: String!
  lastDeployed: DateTime
  status: DeploymentStatus!
}

"""Current status of a fraise in an environment"""
type FraiseStatus {
  fraiseId: String!
  environment: String!
  status: DeploymentStatus!
  lastDeployed: DateTime
  lastError: String
}

"""Deployment event"""
type Deployment {
  id: String!
  fraiseId: String!
  environment: String!
  status: DeploymentStatus!
  startedAt: DateTime!
  completedAt: DateTime
  errorMessage: String
  commitSha: String
  duration: Int  # seconds
}

"""Deployment status"""
enum DeploymentStatus {
  PENDING
  RUNNING
  SUCCESS
  FAILED
}

"""Deployment statistics"""
type DeploymentStatistics {
  fraiseId: String
  period: Int  # days
  totalDeployments: Int!
  successful: Int!
  failed: Int!
  successRate: Float!
  averageDurationSeconds: Float!
  lastDeployment: DateTime
}

"""Webhook event"""
type WebhookEvent {
  id: String!
  provider: GitProvider!
  eventType: String!
  branch: String!
  commitSha: String!
  commitMessage: String
  author: String
  receivedAt: DateTime!
  triggeringFraise: Fraise
  triggeringEnvironment: Environment
}

"""Git provider"""
enum GitProvider {
  GITHUB
  GITLAB
  GITEA
  BITBUCKET
}
```

### Query Root Type

```graphql
type Query {
  """Get a single fraise by ID"""
  fraise(id: String!): Fraise

  """List all fraises"""
  fraises(
    type: FraiseType
    limit: Int = 50
    offset: Int = 0
  ): [Fraise!]!

  """Get deployment by ID"""
  deployment(id: String!): Deployment

  """Get deployment history with filtering"""
  deploymentHistory(
    fraiseId: String
    environment: String
    status: DeploymentStatus
    limit: Int = 50
    offset: Int = 0
  ): [Deployment!]!

  """Get deployment statistics"""
  deploymentStatistics(
    fraiseId: String
    environment: String
    timeRangeDays: Int = 30
  ): DeploymentStatistics!

  """Get webhook events"""
  webhookEvents(
    provider: GitProvider
    limit: Int = 50
  ): [WebhookEvent!]!

  """Get current deployment status for a fraise+environment"""
  fraiseStatus(fraiseId: String!, environment: String!): FraiseStatus
}
```

### Mutation Root Type

```graphql
type Mutation {
  """Trigger a deployment"""
  deploy(
    fraiseId: String!
    environment: String!
    force: Boolean = false
  ): Deployment!

  """Cancel a running deployment"""
  cancelDeployment(deploymentId: String!): Deployment!

  """Retry a failed deployment"""
  retryDeployment(deploymentId: String!): Deployment!

  """Schedule a deployment for later"""
  scheduleDeployment(
    fraiseId: String!
    environment: String!
    scheduledFor: DateTime!
  ): Deployment!
}
```

### Subscription Root Type

```graphql
type Subscription {
  """Subscribe to deployment status changes"""
  deploymentStatusChanged(fraiseId: String): Deployment!

  """Subscribe to webhook events"""
  webhookReceived: WebhookEvent!

  """Subscribe to new deployments"""
  newDeployment(fraiseId: String): Deployment!
}
```

---

## Real-World Usage Examples

### Example 1: Dashboard - Show All Services with Status

```graphql
query DashboardQuery {
  fraises(limit: 100) {
    id
    name
    type
    environments {
      name
      lastDeployed
      status
    }
  }
}
```

**Response:**

```json
{
  "data": {
    "fraises": [
      {
        "id": "api_gateway",
        "name": "API Gateway",
        "type": "API",
        "environments": [
          {
            "name": "production",
            "lastDeployed": "2026-01-15T14:30:00Z",
            "status": "SUCCESS"
          },
          {
            "name": "staging",
            "lastDeployed": "2026-01-15T09:45:00Z",
            "status": "SUCCESS"
          }
        ]
      }
    ]
  }
}
```

### Example 2: Deployment History - Show Recent Failures

```graphql
query RecentFailures {
  deploymentHistory(
    status: FAILED
    limit: 10
  ) {
    id
    fraiseId
    environment
    startedAt
    errorMessage
  }
}
```

### Example 3: Statistics - Show Success Rate

```graphql
query Stats {
  deploymentStatistics(
    fraiseId: "my_api"
    timeRangeDays: 30
  ) {
    totalDeployments
    successful
    failed
    successRate
    averageDurationSeconds
  }
}
```

### Example 4: Trigger Deployment

```graphql
mutation Deploy {
  deploy(
    fraiseId: "my_api"
    environment: "production"
  ) {
    id
    status
    startedAt
  }
}
```

### Example 5: Real-Time Status Updates

```graphql
subscription OnDeploymentChange {
  deploymentStatusChanged(fraiseId: "my_api") {
    id
    status
    completedAt
    errorMessage
  }
}
```

---

## Implementation Architecture

### Database Layer (SQLite)

**Write Tables (Append-Only):**

```sql
CREATE TABLE tb_deployments (
  id TEXT PRIMARY KEY,
  fraise_id TEXT NOT NULL,
  environment TEXT NOT NULL,
  status TEXT NOT NULL,  -- pending, running, success, failed
  started_at TIMESTAMP NOT NULL,
  completed_at TIMESTAMP,
  error_message TEXT,
  commit_sha TEXT
);

CREATE TABLE tb_webhook_events (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  event_type TEXT NOT NULL,
  branch TEXT NOT NULL,
  commit_sha TEXT NOT NULL,
  commit_message TEXT,
  author TEXT,
  received_at TIMESTAMP NOT NULL
);
```

**Read Views (Optimized for Queries):**

```sql
CREATE VIEW v_fraise_status AS
  SELECT DISTINCT ON (fraise_id, environment)
    fraise_id,
    environment,
    status,
    started_at AS last_deployed
  FROM tb_deployments
  ORDER BY fraise_id, environment, started_at DESC;

CREATE VIEW v_deployment_history AS
  SELECT *
  FROM tb_deployments
  ORDER BY started_at DESC;

CREATE VIEW v_deployment_stats AS
  SELECT
    fraise_id,
    COUNT(*) AS total_deployments,
    COUNT(CASE WHEN status = 'success' THEN 1 END) AS successful,
    COUNT(CASE WHEN status = 'failed' THEN 1 END) AS failed,
    CAST(COUNT(CASE WHEN status = 'success' THEN 1 END) AS FLOAT) /
      COUNT(*) AS success_rate,
    AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) AS avg_duration
  FROM tb_deployments
  WHERE started_at > now() - INTERVAL '30 days'
  GROUP BY fraise_id;
```

### FraiseQL Schema Layer (Python)

```python
# fraiseql/fraisier/schema/py/models.py

from fraiseql import type as fraiseql_type
from datetime import datetime
from enum import Enum

class FraiseType(str, Enum):
    API = "api"
    ETL = "etl"
    SCHEDULED = "scheduled"
    BACKUP = "backup"

class DeploymentStatus(str, Enum):
    PENDING = "pending"
    RUNNING = "running"
    SUCCESS = "success"
    FAILED = "failed"

@fraiseql_type
class Deployment:
    """A deployment event"""
    id: str
    fraise_id: str
    environment: str
    status: DeploymentStatus
    started_at: datetime
    completed_at: datetime | None
    error_message: str | None
    commit_sha: str | None

@fraiseql_type
class FraiseStatus:
    """Current status of a fraise in an environment"""
    fraise_id: str
    environment: str
    status: DeploymentStatus
    last_deployed: datetime | None
    last_error: str | None

@fraiseql_type
class Fraise:
    """A deployable service"""
    id: str
    name: str
    type: FraiseType
    description: str | None

    # Resolvers that call database functions/views
    def deployment_history(self, limit: int = 50) -> list[Deployment]:
        # SELECT * FROM v_deployment_history
        # WHERE fraise_id = $1 LIMIT $2
        ...

    def current_status(self, environment: str) -> FraiseStatus:
        # SELECT * FROM v_fraise_status
        # WHERE fraise_id = $1 AND environment = $2
        ...

@fraiseql_type
class Query:
    """Root query type"""

    def fraise(self, id: str) -> Fraise:
        # SELECT * FROM fraises WHERE id = $1
        ...

    def deployment_history(
        self,
        fraise_id: str | None = None,
        environment: str | None = None,
        status: DeploymentStatus | None = None,
        limit: int = 50
    ) -> list[Deployment]:
        # SELECT * FROM v_deployment_history
        # WHERE ... LIMIT $n
        ...

@fraiseql_type
class Mutation:
    """Root mutation type"""

    def deploy(
        self,
        fraise_id: str,
        environment: str,
        force: bool = False
    ) -> Deployment:
        # CALL fn_request_deployment($1, $2, $3)
        ...

    def cancel_deployment(self, deployment_id: str) -> Deployment:
        # CALL fn_cancel_deployment($1)
        ...

@fraiseql_type
class Subscription:
    """Root subscription type"""

    def deployment_status_changed(
        self,
        fraise_id: str | None = None
    ) -> Deployment:
        # LISTEN to channel deployment_changes
        # FILTER WHERE fraise_id = $1
        ...
```

### GraphQL Runtime (Rust)

```
fraiseql-server loads CompiledSchema.json
↓
Routes GraphQL query to compiled executor
↓
Executor maps to database queries/functions
↓
SQLite returns results
↓
Results serialized as JSON response
```

**No custom GraphQL resolver code needed!** All routing happens in the compiled schema.

---

## Migration Path

### Phase 6: Create Schema & Database

1. Define FraiseQL types with @fraiseql decorators
2. Create database views and functions
3. Compile schema: `fraiseql-cli compile schema.json`
4. Test with `fraiseql-server`

### Phase 7: Implement Resolvers

1. Link FraiseQL type fields to database queries
2. Implement query resolvers
3. Implement mutation resolvers
4. Test end-to-end

### Phase 8: Add Subscriptions

1. Implement real-time status updates
2. Use PostgreSQL LISTEN/NOTIFY (if using PostgreSQL)
3. Or polling (if using SQLite)

### Phase 9+: Client Integration

1. Update CLI to use GraphQL API (optional)
2. Create web dashboard
3. Create mobile app
4. Integrate with other services

---

## Comparison: Old vs New Architecture

### Before (Current State)

```
Fraisier CLI          Webhook Server
    ↓                     ↓
  Click CLI          FastAPI REST
    ↓                     ↓
  Python                Python
    ↓                     ↓
SQLite Database (custom queries)
```

**Issues:**

- Custom REST endpoints
- Manual query writing
- No type safety
- Can't serve multiple clients efficiently

### After (GraphQL Future)

```
Fraisier CLI          Web Dashboard        Mobile App
    ↓                     ↓                     ↓
      ↘                 ↙                     ↙
        GraphQL API (FraiseQL)
             ↓
        Compiled Executor
             ↓
        Query Router
             ↓
        Database (SQLite/PostgreSQL)
```

**Benefits:**
✅ Single GraphQL API for all clients
✅ Type-safe queries
✅ Auto-generated resolvers
✅ Consistent, predictable interface
✅ Real-time subscriptions built-in
✅ Demonstrates FraiseQL technology

---

## Key Insight: Fraisier Dogfoods FraiseQL

**Fraisier is the "eating our own dog food" project:**

- **Fraisier deployment status** is served via **FraiseQL GraphQL API**
- **Fraisier webhooks** trigger via **FraiseQL mutations**
- **Fraisier history** is queried via **FraiseQL queries**
- **Real-time updates** via **FraiseQL subscriptions**

This proves that FraiseQL is production-ready and performant for real-world use cases.

---

## Success Criteria

When complete, Fraisier should support:

- ✅ **Query** deployment history via GraphQL
- ✅ **Query** fraise status via GraphQL
- ✅ **Trigger** deployments via GraphQL mutations
- ✅ **Cancel** deployments via GraphQL mutations
- ✅ **Subscribe** to deployment status changes
- ✅ **Access** via CLI, web, and mobile clients
- ✅ **Performance** matches or exceeds current REST API
- ✅ **Type safety** across all interfaces

---

**For detailed implementation steps, see:** `.claude/FRAISIER_ACTION_ITEMS.md` → Phase 5 & 6
