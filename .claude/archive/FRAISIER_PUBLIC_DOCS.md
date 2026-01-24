# Fraisier: The FraiseQL Reference Implementation

**For Public Documentation (to be included in GitHub)**

---

## What is Fraisier?

Fraisier is the **canonical reference implementation** of a production FraiseQL application.

It demonstrates:

- ✅ GraphQL schema definition with `@fraiseql` decorators
- ✅ CQRS pattern for deployment history (write tables + read views)
- ✅ Real-time updates via GraphQL subscriptions
- ✅ Multi-provider webhook support (GitHub, GitLab, Gitea, Bitbucket)
- ✅ Production-ready error handling and validation
- ✅ Complete deployment orchestration workflow

**Location:** `/fraiseql/fraisier/`

---

## Architecture

```
Python Schema Definition (@fraiseql decorators)
    ↓
fraiseql-cli compile
    ↓
schema.compiled.json (Optimized execution plans)
    ↓
fraiseql-server (HTTP endpoint)
    ↓
GraphQL API (Queries, Mutations, Subscriptions)
```

---

## File Structure

```
fraiseql/
└── fraisier/
    ├── fraisier/                    # Python CLI + Webhook Server
    │   ├── cli.py                   # Click CLI interface
    │   ├── webhook.py               # FastAPI webhook server
    │   ├── config.py                # YAML configuration
    │   ├── database.py              # SQLite state management
    │   ├── deployers/               # Deployment strategies
    │   │   ├── api_deployer.py
    │   │   ├── etl_deployer.py
    │   │   ├── scheduled_deployer.py
    │   │   └── backup_deployer.py
    │   └── git/                     # Git provider abstractions
    │       ├── github.py
    │       ├── gitlab.py
    │       ├── gitea.py
    │       └── bitbucket.py
    ├── schema/
    │   └── py/
    │       ├── models.py            # @fraiseql type definitions
    │       ├── enums.py             # Enum types
    │       └── resolvers.py         # Query/Mutation/Subscription resolvers
    ├── db/
    │   ├── schema.sql               # SQLite database schema
    │   ├── views.sql                # Materialized views (CQRS pattern)
    │   └── functions.sql            # Database functions
    ├── tests/
    │   ├── unit/                    # Component tests
    │   ├── integration/             # Workflow tests
    │   └── fixtures/                # Test data
    ├── pyproject.toml               # Python package config
    ├── fraises.example.yaml         # Example configuration
    └── README.md                    # User documentation
```

---

## Key Features

### 1. Deployment Orchestration

**Fraise Types:**

- **API** - Web services with systemd, health checks, database migrations
- **ETL** - Data pipelines and batch processing
- **Scheduled** - Cron jobs via systemd timers
- **Backup** - Database backups with retention policies

**Deployment Workflow:**

1. Developer pushes to Git
2. Webhook sent to Fraisier
3. Signature verified (HMAC-SHA256)
4. Branch mapped to service + environment
5. Deployment strategy executed
6. Health check performed
7. Status recorded in database

### 2. CQRS Database Pattern

**Write Tables (Append-only):**

- `tb_deployments` - Every deployment event
- `tb_webhook_events` - Every webhook received

**Read Views (Optimized):**

- `v_fraise_status` - Current status per environment
- `v_deployment_history` - Full history for queries
- `v_deployment_stats` - Aggregated statistics

**Benefits:**

- Complete audit trail
- Query optimization
- Scalability

### 3. Multi-Provider Support

**Git Providers:**

- GitHub (github.com + GitHub Enterprise)
- GitLab (gitlab.com + self-hosted)
- Gitea (self-hosted + Forgejo)
- Bitbucket (Cloud + Server/Data Center)

Each provider:

- ✅ Signature verification
- ✅ Webhook payload normalization
- ✅ Branch mapping support

### 4. GraphQL API

**Queries:**

```graphql
query {
  fraise(id: "my_api") {
    id
    name
    deploymentHistory(limit: 10) {
      id
      status
      startedAt
    }
  }
}
```

**Mutations:**

```graphql
mutation {
  deploy(fraiseId: "my_api", environment: "production") {
    id
    status
  }
}
```

**Subscriptions:**

```graphql
subscription {
  deploymentStatusChanged(fraiseId: "my_api") {
    status
    completedAt
  }
}
```

---

## Configuration

Create `fraises.yaml`:

```yaml
git:
  provider: github
  github:
    webhook_secret: ${FRAISIER_WEBHOOK_SECRET}

fraises:
  my_api:
    type: api
    description: My GraphQL API
    environments:
      production:
        name: api.example.com
        branch: main
        app_path: /var/www/api
        systemd_service: api.service
        health_check:
          url: https://api.example.com/health

branch_mapping:
  main:
    fraise: my_api
    environment: production
```

---

## Schema Definition

The GraphQL schema is defined using `@fraiseql` decorators:

```python
# fraiseql/fraisier/schema/py/models.py

from fraiseql import type as fraiseql_type
from datetime import datetime
from enum import Enum

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
    completed_at: datetime | None = None
    error_message: str | None = None

@fraiseql_type
class Query:
    def deployment(self, id: str) -> Deployment:
        # Query implementation
        ...

    def deployment_history(
        self,
        fraise_id: str | None = None,
        limit: int = 50
    ) -> list[Deployment]:
        # Query implementation
        ...
```

---

## Running Fraisier

### Installation

```bash
cd fraiseql/fraisier
pip install -e .
```

### CLI Commands

```bash
# List all fraises
fraisier list

# Deploy a service
fraisier deploy my_api production

# Check status
fraisier status my_api production

# View deployment history
fraisier history
```

### Webhook Server

```bash
# Start webhook server (listens on port 8080)
fraisier-webhook

# With custom port
fraisier-webhook --port 3000
```

### GraphQL API

```bash
# Compile schema
fraiseql-cli compile fraiseql/fraisier/schema/py/models.py

# Run server
fraiseql-server --schema schema.compiled.json

# Query via GraphQL
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ deploymentHistory { id status } }"}'
```

---

## Webhook Setup

### GitHub

1. Go to Repository → Settings → Webhooks
2. Click "Add webhook"
3. Payload URL: `https://your-fraisier.com/webhook`
4. Content type: `application/json`
5. Events: Push events
6. Secret: Your webhook secret

### GitLab

1. Go to Project → Settings → Webhooks
2. URL: `https://your-fraisier.com/webhook`
3. Trigger: Push events
4. Secret token: Your webhook secret

### Gitea

1. Go to Repository → Settings → Webhooks
2. Target URL: `https://your-fraisier.com/webhook`
3. HTTP Method: POST
4. Events: Push
5. Secret: Your webhook secret

### Bitbucket

1. Go to Repository → Repository settings → Webhooks
2. Create webhook
3. URL: `https://your-fraisier.com/webhook`
4. Trigger: Push
5. Secret: Your webhook secret

---

## Database Schema

### Tables

```sql
CREATE TABLE tb_deployments (
    id TEXT PRIMARY KEY,
    fraise_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    status TEXT NOT NULL,
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
    received_at TIMESTAMP NOT NULL
);
```

### Views

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
    SELECT * FROM tb_deployments
    ORDER BY started_at DESC;
```

---

## Deployment Strategies

### API Service

```python
# fraisier/deployers/api_deployer.py

1. Git clone/pull
2. Build artifacts (if build.sh exists)
3. Database migration
4. Service restart (systemctl restart SERVICE)
5. Health check (GET /health)
6. Record result
```

### ETL Pipeline

```python
# fraisier/deployers/etl_deployer.py

1. Git clone/pull
2. Execute script (script_path)
3. Log output
4. Handle errors
5. Send notifications
```

### Scheduled Job

```python
# fraisier/deployers/scheduled_deployer.py

1. Create systemd service
2. Create systemd timer
3. Set schedule (cron format)
4. Enable timer
```

### Backup Job

```python
# fraisier/deployers/backup_deployer.py

1. Create backup script
2. Set retention policy
3. Configure remote sync (optional)
4. Enable timer
```

---

## Integration with FraiseQL

Fraisier demonstrates FraiseQL's capabilities:

**Schema Authoring:** `@fraiseql` decorators in Python

**Compilation:** `fraiseql-cli compile` generates optimized execution plans

**Runtime:** `fraiseql-server` serves GraphQL API

**Result:** Production-grade GraphQL application with zero custom server code

---

## Modifying Fraisier

### Add a New Query

1. Edit `schema/py/models.py`
2. Add method to `Query` class
3. Implement resolver logic
4. Recompile: `fraiseql-cli compile schema/py/models.py`
5. Test: `fraiseql-server --schema schema.compiled.json`

### Add a New Fraise Type

1. Edit `fraises.yaml`
2. Define new service type
3. Create deployer in `deployers/`
4. Add mapping in `branch_mapping`

### Add a New Mutation

1. Edit `schema/py/models.py`
2. Add method to `Mutation` class
3. Implement resolver logic
4. Recompile schema

---

## Performance

Fraisier uses FraiseQL's compiled execution engine:

- ✅ **Zero-runtime overhead** - Schema compiled to optimized SQL
- ✅ **Type safety** - All queries/mutations type-checked at compile time
- ✅ **Scalable** - SQLite for local, PostgreSQL for production
- ✅ **Real-time** - Subscriptions for instant updates
- ✅ **Efficient** - Materialized views for fast queries

---

## Troubleshooting

**Webhook not triggering:**

- Check webhook delivery in Git provider
- Verify webhook secret matches
- Check Fraisier logs: `journalctl -u fraisier-webhook`

**Deployment failed:**

- Check: `fraisier status my_api prod`
- View history: `fraisier history my_api`
- Check deployment logs

**Health check timeout:**

- Increase timeout in `fraises.yaml`
- Verify service is healthy: `curl /health`
- Check service logs: `systemctl status api.service`

---

## Contributing

To contribute to Fraisier:

1. Fork the repository
2. Create a feature branch
3. Make changes to schema, resolvers, or deployers
4. Write tests
5. Submit pull request

Changes to the schema:

- Edit `schema/py/models.py`
- Recompile: `fraiseql-cli compile schema/py/models.py`
- Test thoroughly

---

## License

MIT License - See LICENSE file

---

## Resources

- [FraiseQL Documentation](https://fraiseql.dev)
- [GraphQL Specification](https://spec.graphql.org)
- [systemd Documentation](https://systemd.io)
