# Multi-Tenant SaaS with Row-Level Security

**Status:** ✅ Production Ready
**Complexity:** ⭐⭐⭐⭐ (Advanced)
**Audience:** SaaS architects, backend developers
**Reading Time:** 30-35 minutes
**Last Updated:** 2026-02-05

Complete guide to building a production-grade multi-tenant SaaS application using FraiseQL with row-level security (RLS).

---

## Architecture Overview

```text
┌─────────────────────────────────────────────────────────┐
│                   Web/Mobile Clients                     │
│          (React, Vue, Flutter, React Native)             │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ↓ (GraphQL queries with JWT)
┌─────────────────────────────────────────────────────────┐
│              FraiseQL Server (Rust)                      │
│  - Extracts tenant_id from JWT                          │
│  - Injects tenant_id into all queries                   │
│  - Handles authentication/authorization                 │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ↓
┌─────────────────────────────────────────────────────────┐
│          PostgreSQL Database with RLS                    │
│  Row-Level Security Policies:                            │
│  - WHERE tenant_id = user.tenant_id                      │
│  - Data isolation at database level                      │
│  - Prevents data leakage even if app is compromised      │
└─────────────────────────────────────────────────────────┘
```text

---

## Schema Design

### Core Tables

```sql
-- Tenants (the SaaS customers)
CREATE TABLE tenants (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  slug VARCHAR(255) UNIQUE NOT NULL, -- mycompany.example.com
  name VARCHAR(255) NOT NULL,
  plan VARCHAR(50) NOT NULL, -- free, starter, pro, enterprise
  stripe_customer_id VARCHAR(255),
  status VARCHAR(50) NOT NULL, -- active, suspended, cancelled
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW()
);

-- Users (tenant members)
CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  email VARCHAR(255) NOT NULL,
  password_hash VARCHAR(255) NOT NULL,
  full_name VARCHAR(255),
  role VARCHAR(50) NOT NULL, -- owner, admin, member, viewer
  status VARCHAR(50) NOT NULL, -- active, invited, deactivated
  last_login TIMESTAMP,
  created_at TIMESTAMP DEFAULT NOW(),

  UNIQUE(tenant_id, email),
  INDEX idx_tenant_id (tenant_id)
);

-- Projects (tenant workspace items)
CREATE TABLE projects (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  name VARCHAR(255) NOT NULL,
  description TEXT,
  owner_id UUID NOT NULL REFERENCES users(id),
  status VARCHAR(50) NOT NULL, -- active, archived
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_tenant_id (tenant_id),
  INDEX idx_owner_id (owner_id)
);

-- Project Members (who can access each project)
CREATE TABLE project_members (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role VARCHAR(50) NOT NULL, -- editor, viewer, admin
  invited_at TIMESTAMP DEFAULT NOW(),
  joined_at TIMESTAMP,

  UNIQUE(project_id, user_id),
  INDEX idx_project_id (project_id),
  INDEX idx_user_id (user_id)
);

-- Tasks (project work items)
CREATE TABLE tasks (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  title VARCHAR(255) NOT NULL,
  description TEXT,
  status VARCHAR(50) NOT NULL, -- todo, in_progress, done
  assigned_to UUID REFERENCES users(id),
  priority VARCHAR(50) NOT NULL, -- low, medium, high
  due_date DATE,
  created_by UUID NOT NULL REFERENCES users(id),
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_tenant_id (tenant_id),
  INDEX idx_project_id (project_id),
  INDEX idx_assigned_to (assigned_to),
  INDEX idx_status (status)
);

-- Audit Log (compliance & debugging)
CREATE TABLE audit_logs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  user_id UUID REFERENCES users(id),
  entity_type VARCHAR(50) NOT NULL, -- users, projects, tasks, etc.
  entity_id UUID NOT NULL,
  action VARCHAR(50) NOT NULL, -- created, updated, deleted
  old_values JSONB,
  new_values JSONB,
  ip_address INET,
  user_agent TEXT,
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_tenant_id (tenant_id),
  INDEX idx_user_id (user_id),
  INDEX idx_created_at (created_at)
);

-- Subscriptions (usage tracking for billing)
CREATE TABLE subscriptions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id UUID NOT NULL UNIQUE REFERENCES tenants(id) ON DELETE CASCADE,
  stripe_subscription_id VARCHAR(255),
  plan VARCHAR(50) NOT NULL,
  status VARCHAR(50) NOT NULL, -- active, past_due, cancelled
  current_period_start DATE,
  current_period_end DATE,
  cancel_at_period_end BOOLEAN DEFAULT FALSE,
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_tenant_id (tenant_id)
);

-- Usage Metrics (for billing)
CREATE TABLE usage_metrics (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  metric_name VARCHAR(100) NOT NULL, -- api_calls, storage_gb, etc.
  metric_value DECIMAL(15, 2),
  period_start DATE NOT NULL,
  period_end DATE NOT NULL,
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_tenant_id (tenant_id),
  INDEX idx_period (period_start, period_end)
);
```text

---

## Row-Level Security Policies

### Enable RLS

```sql
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE projects ENABLE ROW LEVEL SECURITY;
ALTER TABLE tasks ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;
ALTER TABLE subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE usage_metrics ENABLE ROW LEVEL SECURITY;
```text

### Tenant Context Function

```sql
-- Get current tenant from JWT claims
CREATE OR REPLACE FUNCTION current_tenant_id() RETURNS UUID AS $$
  SELECT (current_setting('app.tenant_id', true))::UUID;
$$ LANGUAGE SQL STABLE;

-- Get current user from JWT claims
CREATE OR REPLACE FUNCTION current_user_id() RETURNS UUID AS $$
  SELECT (current_setting('app.user_id', true))::UUID;
$$ LANGUAGE SQL STABLE;

-- Get current user's role
CREATE OR REPLACE FUNCTION current_user_role() RETURNS TEXT AS $$
  SELECT (current_setting('app.user_role', true))::TEXT;
$$ LANGUAGE SQL STABLE;
```text

### RLS Policies - Users Table

```sql
-- Users can only see users in their tenant
CREATE POLICY users_tenant_isolation ON users
  FOR SELECT
  USING (tenant_id = current_tenant_id());

-- Users can only update their own profile
CREATE POLICY users_self_update ON users
  FOR UPDATE
  USING (
    id = current_user_id() OR
    current_user_role() IN ('owner', 'admin')
  );

-- Only admins can delete users
CREATE POLICY users_delete ON users
  FOR DELETE
  USING (current_user_role() IN ('owner', 'admin'));

-- Allow new user creation (signup)
CREATE POLICY users_insert ON users
  FOR INSERT
  WITH CHECK (true); -- Allow insertion, FraiseQL handles tenant assignment
```text

### RLS Policies - Projects Table

```sql
-- Users can only see projects in their tenant
-- AND either they're the owner or a project member
CREATE POLICY projects_visibility ON projects
  FOR SELECT
  USING (
    tenant_id = current_tenant_id() AND
    (
      owner_id = current_user_id() OR
      id IN (
        SELECT project_id FROM project_members
        WHERE user_id = current_user_id()
      )
    )
  );

-- Only project owners and admins can update
CREATE POLICY projects_update ON projects
  FOR UPDATE
  USING (
    owner_id = current_user_id() OR
    current_user_role() IN ('owner', 'admin')
  );

-- Only owners can delete
CREATE POLICY projects_delete ON projects
  FOR DELETE
  USING (owner_id = current_user_id());

-- Can create in own tenant
CREATE POLICY projects_insert ON projects
  FOR INSERT
  WITH CHECK (tenant_id = current_tenant_id());
```text

### RLS Policies - Tasks Table

```sql
-- Users can only see tasks in projects they have access to
CREATE POLICY tasks_visibility ON tasks
  FOR SELECT
  USING (
    tenant_id = current_tenant_id() AND
    project_id IN (
      SELECT id FROM projects
      WHERE owner_id = current_user_id()
         OR id IN (SELECT project_id FROM project_members WHERE user_id = current_user_id())
    )
  );

-- Project members and admins can update tasks
CREATE POLICY tasks_update ON tasks
  FOR UPDATE
  USING (
    project_id IN (
      SELECT id FROM projects
      WHERE owner_id = current_user_id()
         OR id IN (SELECT project_id FROM project_members WHERE user_id = current_user_id())
    ) OR
    current_user_role() IN ('owner', 'admin')
  );

-- Only project owners can delete tasks
CREATE POLICY tasks_delete ON tasks
  FOR DELETE
  USING (
    project_id IN (
      SELECT id FROM projects WHERE owner_id = current_user_id()
    ) OR
    current_user_role() IN ('owner', 'admin')
  );
```text

### RLS Policies - Audit Logs

```sql
-- Users can only see audit logs for their tenant
CREATE POLICY audit_logs_visibility ON audit_logs
  FOR SELECT
  USING (
    tenant_id = current_tenant_id() AND
    current_user_role() IN ('owner', 'admin') -- Only admins see logs
  );

-- Prevent direct manipulation of audit logs
CREATE POLICY audit_logs_immutable ON audit_logs
  FOR UPDATE, DELETE
  USING (false);
```text

---

## FraiseQL Schema Definition (Python)

```python
# schema.py
from fraiseql import types, authorize
from datetime import datetime

@types.object
class Tenant:
    id: str
    slug: str
    name: str
    plan: str
    status: str
    users: list['User']
    projects: list['Project']
    subscription: 'Subscription'
    created_at: datetime

@types.object
class User:
    id: str
    email: str
    full_name: str
    role: str  # owner, admin, member, viewer
    status: str
    tenant: Tenant
    projects: list['Project']  # Projects this user is a member of
    tasks: list['Task']  # Tasks assigned to this user
    created_at: datetime

@types.object
class Project:
    id: str
    name: str
    description: str
    owner: User
    status: str
    members: list[User]
    tasks: list['Task']
    created_at: datetime

@types.object
class Task:
    id: str
    title: str
    description: str
    status: str
    assigned_to: User | None
    project: Project
    priority: str
    due_date: str | None
    created_by: User
    created_at: datetime

@types.object
class Subscription:
    id: str
    plan: str
    status: str
    current_period_start: str
    current_period_end: str
    stripe_subscription_id: str | None

@types.object
class Query:
    @authorize(roles=['owner', 'admin', 'member', 'viewer'])
    def me(self) -> User:
        """Current authenticated user"""
        pass

    @authorize(roles=['owner', 'admin', 'member'])
    def users(self, limit: int = 50, offset: int = 0) -> list[User]:
        """List all users in current tenant"""
        pass

    @authorize(roles=['owner', 'admin', 'member', 'viewer'])
    def projects(self, status: str | None = None) -> list[Project]:
        """List accessible projects"""
        pass

    @authorize(roles=['owner', 'admin', 'member', 'viewer'])
    def tasks(
        self,
        project_id: str,
        status: str | None = None,
        assigned_to: str | None = None,
        limit: int = 50,
        offset: int = 0
    ) -> list[Task]:
        """List tasks in a project"""
        pass

    @authorize(roles=['owner', 'admin'])
    def usage_report(self, period_start: str, period_end: str) -> dict:
        """Get usage metrics for current tenant"""
        pass

    @authorize(roles=['owner', 'admin'])
    def audit_logs(self, limit: int = 100, offset: int = 0) -> list[dict]:
        """Get audit logs (admin only)"""
        pass

@types.object
class Mutation:
    @authorize(roles=['owner', 'admin'])
    def invite_user(self, email: str, role: str = 'member') -> User:
        """Send invitation to new user"""
        pass

    @authorize(roles=['owner', 'admin'])
    def create_project(self, name: str, description: str = '') -> Project:
        """Create new project"""
        pass

    @authorize(roles=['owner', 'admin', 'member'])
    def create_task(
        self,
        project_id: str,
        title: str,
        description: str = '',
        assigned_to: str | None = None,
        priority: str = 'medium'
    ) -> Task:
        """Create task in project"""
        pass

    @authorize(roles=['owner', 'admin', 'member'])
    def update_task(
        self,
        task_id: str,
        status: str | None = None,
        assigned_to: str | None = None
    ) -> Task:
        """Update task status/assignment"""
        pass

    @authorize(roles=['owner'])
    def upgrade_plan(self, plan: str, stripe_token: str) -> Subscription:
        """Upgrade subscription plan"""
        pass
```text

---

## JWT Token Structure

### Token Payload

```json
{
  "sub": "user_123",
  "email": "alice@company.com",
  "tenant_id": "tenant_456",
  "user_id": "user_123",
  "role": "admin",
  "iat": 1640000000,
  "exp": 1640086400
}
```text

### Setting Tenant Context in FraiseQL Server

```rust
// fraiseql-server middleware (pseudo-code)
async fn set_tenant_context(token: &Claims) -> Result<()> {
    // Set tenant and user context for RLS policies
    client.execute(
        "SET app.tenant_id = $1",
        &[&token.tenant_id]
    ).await?;

    client.execute(
        "SET app.user_id = $1",
        &[&token.user_id]
    ).await?;

    client.execute(
        "SET app.user_role = $1",
        &[&token.role]
    ).await?;

    Ok(())
}
```text

---

## Client Implementation

### React Hook for Current Tenant

```typescript
import { useQuery, gql } from '@apollo/client';

const ME_QUERY = gql`
  query Me {
    me {
      id
      email
      full_name
      role
      tenant {
        id
        slug
        name
        plan
      }
    }
  }
`;

export function useCurrentUser() {
  const { data, loading, error } = useQuery(ME_QUERY);
  return {
    user: data?.me,
    tenant: data?.me?.tenant,
    loading,
    error,
  };
}
```text

### List Projects Query

```typescript
const LIST_PROJECTS = gql`
  query ListProjects {
    projects {
      id
      name
      owner {
        full_name
      }
      members {
        id
        email
      }
    }
  }
`;

export function ProjectList() {
  const { data, loading } = useQuery(LIST_PROJECTS);

  if (loading) return <div>Loading...</div>;

  return (
    <ul>
      {data?.projects?.map((project) => (
        <li key={project.id}>
          <h3>{project.name}</h3>
          <p>Owner: {project.owner.full_name}</p>
          <p>Members: {project.members.length}</p>
        </li>
      ))}
    </ul>
  );
}
```text

### Create Project Mutation

```typescript
const CREATE_PROJECT = gql`
  mutation CreateProject($name: String!, $description: String!) {
    createProject(name: $name, description: $description) {
      id
      name
      description
      owner {
        id
        full_name
      }
    }
  }
`;

export function CreateProjectForm() {
  const [name, setName] = useState('');
  const [createProject, { loading }] = useMutation(CREATE_PROJECT, {
    refetchQueries: [{ query: LIST_PROJECTS }],
  });

  const handleCreate = async () => {
    await createProject({
      variables: { name, description: '' },
    });
    setName('');
  };

  return (
    <form onSubmit={(e) => { e.preventDefault(); handleCreate(); }}>
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Project name"
      />
      <button disabled={loading}>Create</button>
    </form>
  );
}
```text

---

## Billing Integration

### Track Usage

```sql
-- Function to increment usage
CREATE OR REPLACE FUNCTION increment_usage(
  p_tenant_id UUID,
  p_metric_name VARCHAR,
  p_amount DECIMAL
) RETURNS void AS $$
BEGIN
  INSERT INTO usage_metrics (tenant_id, metric_name, metric_value, period_start, period_end)
  VALUES (
    p_tenant_id,
    p_metric_name,
    p_amount,
    DATE_TRUNC('month', NOW())::DATE,
    DATE_TRUNC('month', NOW() + INTERVAL '1 month')::DATE - INTERVAL '1 day'
  )
  ON CONFLICT (tenant_id, metric_name, period_start, period_end)
  DO UPDATE SET metric_value = metric_value + EXCLUDED.metric_value;
END;
$$ LANGUAGE plpgsql;
```text

### Check Usage Limits

```sql
-- Example: Check if tenant has exceeded API call limit
CREATE OR REPLACE FUNCTION check_api_limit(p_tenant_id UUID) RETURNS BOOLEAN AS $$
DECLARE
  v_current_usage DECIMAL;
  v_plan VARCHAR;
  v_limit DECIMAL;
BEGIN
  SELECT plan INTO v_plan FROM tenants WHERE id = p_tenant_id;

  SELECT metric_value INTO v_current_usage FROM usage_metrics
  WHERE tenant_id = p_tenant_id
    AND metric_name = 'api_calls'
    AND period_start = DATE_TRUNC('month', NOW())::DATE;

  v_current_usage := COALESCE(v_current_usage, 0);

  -- Define limits per plan
  v_limit := CASE v_plan
    WHEN 'free' THEN 1000
    WHEN 'starter' THEN 10000
    WHEN 'pro' THEN 100000
    WHEN 'enterprise' THEN 999999999
  END;

  RETURN v_current_usage < v_limit;
END;
$$ LANGUAGE plpgsql;
```text

---

## Security Considerations

### 1. Token Validation

```typescript
// Verify JWT before setting context
function validateToken(token: string): Claims | null {
  try {
    return jwt.verify(token, process.env.JWT_SECRET) as Claims;
  } catch (err) {
    console.error('Invalid token:', err);
    return null;
  }
}
```text

### 2. Prevent Tenant ID Forgery

```typescript
// Never trust tenant_id from client - get it from token
const getTenantIdFromToken = (token: Claims): string => {
  if (!token.tenant_id) {
    throw new Error('Invalid token: missing tenant_id');
  }
  return token.tenant_id;
};
```text

### 3. Audit All Changes

```sql
-- Trigger to auto-log changes
CREATE OR REPLACE FUNCTION audit_trigger() RETURNS TRIGGER AS $$
BEGIN
  INSERT INTO audit_logs (
    tenant_id,
    user_id,
    entity_type,
    entity_id,
    action,
    old_values,
    new_values
  ) VALUES (
    current_tenant_id(),
    current_user_id(),
    TG_TABLE_NAME,
    CASE WHEN TG_OP = 'DELETE' THEN OLD.id ELSE NEW.id END,
    TG_OP,
    CASE WHEN TG_OP = 'DELETE' THEN row_to_json(OLD) ELSE NULL END,
    CASE WHEN TG_OP IN ('INSERT', 'UPDATE') THEN row_to_json(NEW) ELSE NULL END
  );
  RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$ LANGUAGE plpgsql;

-- Apply trigger to all tables
CREATE TRIGGER users_audit AFTER INSERT OR UPDATE OR DELETE ON users
FOR EACH ROW EXECUTE FUNCTION audit_trigger();

CREATE TRIGGER projects_audit AFTER INSERT OR UPDATE OR DELETE ON projects
FOR EACH ROW EXECUTE FUNCTION audit_trigger();

CREATE TRIGGER tasks_audit AFTER INSERT OR UPDATE OR DELETE ON tasks
FOR EACH ROW EXECUTE FUNCTION audit_trigger();
```text

---

## Scaling Considerations

### Connection Pooling

- Use PgBouncer for connection pooling
- Set pool size to 100-200 per tenant segment
- Use transaction pooling mode for high-concurrency scenarios

### Caching

- Cache tenant configuration (plan limits, features)
- Cache user roles (expires after 5 minutes)
- Invalidate on role/permission changes

### Monitoring

- Track per-tenant query performance
- Monitor RLS policy evaluation time
- Alert on unusual usage patterns

### Multi-Region Deployment

```text
Region 1 (US-East)           Region 2 (EU)
    ↓                             ↓
FraiseQL Server ----------- Replication -------- FraiseQL Server
    ↓                             ↓
PostgreSQL Primary ---------- Streaming -------- PostgreSQL Replica
(tenant_a, tenant_b)         (standby)         (for reads)
```text

---

## Testing Multi-Tenant Isolation

```typescript
describe('Multi-Tenant Isolation', () => {
  it('tenant_a cannot see tenant_b data', async () => {
    // Login as user from tenant_a
    const token_a = generateToken({ tenant_id: 'a', user_id: 'user_1' });

    // Try to query with tenant_b context (should fail)
    const result = await client.query(LIST_PROJECTS, {
      headers: { authorization: `Bearer ${token_a}` },
    });

    // RLS policy should prevent access
    expect(result.errors).toBeDefined();
  });

  it('users can only see their tenant projects', async () => {
    const token = generateToken({ tenant_id: 'a', user_id: 'user_1' });
    const result = await client.query(LIST_PROJECTS, {
      headers: { authorization: `Bearer ${token}` },
    });

    // All returned projects should have tenant_id = 'a'
    expect(result.data.projects.every(p => p.tenant_id === 'a')).toBe(true);
  });
});
```text

---

## Common Pitfalls

### ❌ Storing tenant_id in app, not JWT

Vulnerable to token swapping. Always extract from verified token.

### ❌ Relying only on app-level filtering

Use database RLS as defense in depth.

### ❌ Not auditing sensitive operations

Track who accessed what, when, for compliance.

### ❌ Same schema for all tenants

Multi-schema is better if tenants are large, separate instances.

---

## See Also

**Related Patterns:**

- [Database Federation](./federation-patterns.md) - Multiple databases
- [Real-Time Collaboration](./realtime-collaboration.md) - Live updates
- [E-Commerce Workflows](./ecommerce-workflows.md) - Complex workflows

**Security Guides:**

- [Production Security Checklist](../guides/production-security-checklist.md)
- [Authentication & Authorization](../guides/authorization-quick-start.md)
- [Audit Logging](../guides/observers.md)

**Deployment:**

- [Production Deployment](../guides/production-deployment.md)
- [Kubernetes Setup](../guides/production-deployment.md)

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
