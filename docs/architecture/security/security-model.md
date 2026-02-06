<!-- Skip to main content -->
---

title: FraiseQL Security Model: Authorization, Row-Level Security, Field Masking, and Audit
description: FraiseQL security operates on five pillars:
keywords: ["design", "scalability", "performance", "patterns", "security"]
tags: ["documentation", "reference"]
---

# FraiseQL Security Model: Authorization, Row-Level Security, Field Masking, and Audit

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Security architects, compliance engineers, application developers, operations teams

---

## Executive Summary

FraiseQL security operates on five pillars:

1. **Authentication** — Verify user identity (external responsibility)
2. **Authorization** — Control what users can do (compile-time + runtime)
3. **Row-Level Security (RLS)** — Filter data per user (database-enforced)
4. **Field-Level Masking** — Hide sensitive fields per user (runtime-enforced)
5. **Audit Logging** — Track who did what and when (durable event log)

**Core principle**: Security is declarative. Developers declare rules; FraiseQL enforces them at compile-time and runtime.

**Security guarantees:**

- ✅ **Compile-time validation** — Authorization rules checked at schema compilation
- ✅ **Runtime enforcement** — Authorization rules re-evaluated on every request
- ✅ **No resolver bypasses** — No way to fetch data that authorization denies
- ✅ **Deterministic** — Same inputs always produce same authorization result
- ✅ **Auditable** — All access attempts logged

### Security Pipeline

**Diagram: Security Architecture** - Multi-layer security pipeline from request to response

```d2
<!-- Code example in D2 Diagram -->
direction: right

Request: "GraphQL Request\n(with JWT token)" {
  shape: box
  style.fill: "#e3f2fd"
}

Authn: "1. Authentication\n(Verify identity)" {
  shape: box
  style.fill: "#f3e5f5"
}

QueryAuth: "2. Query Authorization\n(Check operation allowed)" {
  shape: box
  style.fill: "#fff3e0"
}

RLS: "3. Row-Level Security\n(Filter database results)" {
  shape: box
  style.fill: "#f1f8e9"
}

FieldAuth: "4. Field Masking\n(Hide sensitive fields)" {
  shape: box
  style.fill: "#ffe0b2"
}

Audit: "5. Audit Logging\n(Record access)" {
  shape: box
  style.fill: "#ffccbc"
}

Response: "Response\n(Authorized data)" {
  shape: box
  style.fill: "#c8e6c9"
}

Denied: "❌ Access Denied" {
  shape: box
  style.fill: "#ffebee"
}

Request -> Authn
Authn -> QueryAuth: "User context"
Authn -> Denied: "Token invalid"
QueryAuth -> RLS: "Operation allowed"
QueryAuth -> Denied: "Operation denied"
RLS -> FieldAuth: "Row-filtered data"
FieldAuth -> Audit: "Masked fields"
Audit -> Response: "Log recorded"
```text
<!-- Code example in TEXT -->

---

## 1. Authentication Context

### 1.1 User Context

Every request carries user context:

```python
<!-- Code example in Python -->
class UserContext:
    user_id: str                    # "user-456"
    username: str                   # "alice@company.com"
    roles: list[str]                # ["user", "member", "team-lead"]
    groups: list[str]               # ["engineering", "frontend-team"]
    permissions: set[str]           # {"read:posts", "write:own:posts"}
    organization_id: str            # "org-123"
    environment: str                # "production"
    authenticated_at: datetime       # When user was authenticated
    token_expires_at: datetime       # When token expires
    metadata: dict                   # Custom metadata {"department": "engineering"}
```text
<!-- Code example in TEXT -->

### 1.2 Context Binding

User context is bound at request time:

```rust
<!-- Code example in RUST -->
// Request arrives with JWT token
let token = extract_bearer_token(&request)?;

// Verify token (external auth provider)
let claims = verify_jwt(token)?;

// Build user context
let user_context = UserContext {
    user_id: claims.sub,
    username: claims.email,
    roles: claims.roles,
    groups: claims.groups,
    organization_id: claims.org_id,
    authenticated_at: claims.iat,
    token_expires_at: claims.exp,
    metadata: claims.metadata,
};

// Bind to request
request.user_context = user_context;

// Pass to query/mutation execution
execute_query(&query, &user_context).await?
```text
<!-- Code example in TEXT -->

### 1.3 Context Immutability

User context is **read-only** and **immutable** during request:

```rust
<!-- Code example in RUST -->
// User context set once per request
request.user_context = authenticate(&token)?;

// Cannot be changed during query execution
// Cannot be escalated to admin role
// Cannot be swapped for another user

// Attempting to modify context raises error
// E_AUTH_CONTEXT_TAMPER_501
```text
<!-- Code example in TEXT -->

---

## 2. Authorization: Compile-Time Rules

### 2.1 Type-Level Authorization

Authorize access to entire type:

```python
<!-- Code example in Python -->
@FraiseQL.type
@FraiseQL.authorize(rule="authenticated")  # Only logged-in users
class Post:
    id: ID
    title: str
    content: str

@FraiseQL.type
@FraiseQL.authorize(rule="admin_only")  # Only admins
class AdminPanel:
    id: ID
    system_logs: [str]
    user_list: [User]

@FraiseQL.type
@FraiseQL.authorize(rule="public")  # Anyone (default)
class Product:
    id: ID
    name: str
    price: float
```text
<!-- Code example in TEXT -->

**Compile-time validation:**

```python
<!-- Code example in Python -->
# During compilation:
# 1. Check type-level authorization exists
# 2. Validate authorization rule is defined
# 3. Validate rule has SQL WHERE clause equivalent
# 4. Validate all fields inherit type authorization
```text
<!-- Code example in TEXT -->

**Runtime effect:**

```graphql
<!-- Code example in GraphQL -->
# Query: Get admin panel data
query {
  adminPanel {
    systemLogs
  }
}

# If user not admin:
{
  "errors": [{
    "message": "Access denied: You don't have permission to access AdminPanel",
    "code": "E_AUTH_PERMISSION_401",
    "reason": "admin_only rule requires role 'admin'"
  }],
  "data": null
}
```text
<!-- Code example in TEXT -->

### 2.2 Field-Level Authorization

Authorize access to individual fields:

```python
<!-- Code example in Python -->
@FraiseQL.type
class User:
    id: ID                          # Public
    username: str                   # Public

    @FraiseQL.authorize(rule="owner_or_admin")
    email: str                      # Only owner or admin can read

    @FraiseQL.authorize(rule="admin_only")
    ssn: str                        # Only admin can read

    @FraiseQL.authorize(rule="own_profile")
    encrypted_password_hash: str    # Only owner can read

@FraiseQL.type
class Post:
    id: ID                          # Public
    title: str                      # Public

    @FraiseQL.authorize(rule="published_or_author")
    content: str                    # Published posts or author
```text
<!-- Code example in TEXT -->

**Compile-time validation:**

```python
<!-- Code example in Python -->
# During compilation:
# 1. Check field authorization rule is defined
# 2. Validate rule references valid user context
# 3. Validate rule SQL clause is safe
# 4. Check for conflicting rules on same field
```text
<!-- Code example in TEXT -->

**Runtime effect:**

When field is unauthorized:

```graphql
<!-- Code example in GraphQL -->
query GetUser($id: ID!) {
  user(id: $id) {
    id              # ✅ Allowed
    username        # ✅ Allowed
    email           # ⚠️ Check authorization
    ssn             # ⚠️ Check authorization
  }
}

# If user is owner of user-123:
{
  "data": {
    "user": {
      "id": "user-123",
      "username": "alice",
      "email": "alice@company.com",   # ✅ Owner can read
      "ssn": null                      # ⚠️ Only admin can read (user not admin)
    }
  }
}

# If user is NOT owner and NOT admin of user-123:
{
  "data": {
    "user": {
      "id": "user-123",
      "username": "alice",
      "email": null,    # ⚠️ Not owner and not admin
      "ssn": null       # ⚠️ Not admin
    }
  }
}
```text
<!-- Code example in TEXT -->

### 2.3 Mutation-Level Authorization

Authorize mutations (write operations):

```python
<!-- Code example in Python -->
@FraiseQL.mutation
@FraiseQL.authorize(rule="authenticated")  # Anyone authenticated can create
def create_post(input: CreatePostInput) -> Post:
    """Create a new post"""
    pass

@FraiseQL.mutation
@FraiseQL.authorize(rule="own_post")  # Can only update own posts
def update_post(id: ID!, input: UpdatePostInput) -> Post:
    """Update a post"""
    pass

@FraiseQL.mutation
@FraiseQL.authorize(rule="admin_only")  # Only admin can delete
def delete_post(id: ID!) -> Boolean:
    """Delete a post"""
    pass
```text
<!-- Code example in TEXT -->

**Authorization evaluation:**

**Diagram: Security Architecture** - Multi-layer security pipeline from request to response

```d2
<!-- Code example in D2 Diagram -->
direction: down

CreateReq: "Create Post Request" {
  shape: box
  style.fill: "#e3f2fd"
}

CreateCheck: "Is user\nauthenticated?" {
  shape: diamond
  style.fill: "#fff9c4"
}

CreateAllow: "✅ Allow\n(Create new post)" {
  shape: box
  style.fill: "#c8e6c9"
}

CreateDeny: "❌ Deny\n(E_AUTH_PERMISSION_401)" {
  shape: box
  style.fill: "#ffebee"
}

UpdateReq: "Update Post Request" {
  shape: box
  style.fill: "#e3f2fd"
}

UpdateCheck: "Does user\nown post?" {
  shape: diamond
  style.fill: "#fff9c4"
}

UpdateAllow: "✅ Allow\n(Update own post)" {
  shape: box
  style.fill: "#c8e6c9"
}

UpdateDeny: "❌ Deny\n(E_AUTH_PERMISSION_401)" {
  shape: box
  style.fill: "#ffebee"
}

DeleteReq: "Delete Post Request" {
  shape: box
  style.fill: "#e3f2fd"
}

DeleteCheck: "Is user\nadmin?" {
  shape: diamond
  style.fill: "#fff9c4"
}

DeleteAllow: "✅ Allow\n(Delete post)" {
  shape: box
  style.fill: "#c8e6c9"
}

DeleteDeny: "❌ Deny\n(E_AUTH_PERMISSION_401)" {
  shape: box
  style.fill: "#ffebee"
}

CreateReq -> CreateCheck
CreateCheck -> CreateAllow: "Yes"
CreateCheck -> CreateDeny: "No"

UpdateReq -> UpdateCheck
UpdateCheck -> UpdateAllow: "Yes"
UpdateCheck -> UpdateDeny: "No"

DeleteReq -> DeleteCheck
DeleteCheck -> DeleteAllow: "Yes"
DeleteCheck -> DeleteDeny: "No"
```text
<!-- Code example in TEXT -->

---

## 3. Row-Level Security (RLS)

### 3.1 RLS Rules

Row-level security filters database results based on user context:

```python
<!-- Code example in Python -->
@FraiseQL.type
@FraiseQL.rls(
    rule="same_organization"
    # Only return posts from user's organization
)
class Post:
    id: ID
    title: str
    organization_id: str  # Part of RLS rule

@FraiseQL.type
@FraiseQL.rls(
    rule="owner_or_admin"
    # Return own records or if user is admin
)
class User:
    id: ID
    username: str
    email: str

@FraiseQL.type
@FraiseQL.rls(
    rule="none"  # No RLS, return all records (if authorized)
)
class PublicProduct:
    id: ID
    name: str
```text
<!-- Code example in TEXT -->

### 3.2 RLS Rule Definition

RLS rules are expressed as SQL WHERE clauses:

```python
<!-- Code example in Python -->
# Built-in RLS rule: same_organization
RLS_RULE_SAME_ORGANIZATION = """
  organization_id = $current_user_organization_id
"""

# Built-in RLS rule: owner_or_admin
RLS_RULE_OWNER_OR_ADMIN = """
  user_id = $current_user_id OR $current_user_role = 'admin'
"""

# Custom RLS rule: department_lead can see department employees
RLS_RULE_DEPARTMENT = """
  department = $current_user_department OR $current_user_role = 'admin'
"""

# Custom RLS rule: complex multi-tenant with team access
RLS_RULE_TEAM_ACCESS = """
  (
    organization_id = $current_user_organization_id
    AND (
      user_id = $current_user_id
      OR team_id = ANY($current_user_team_ids)
      OR $current_user_role = 'admin'
    )
  )
"""
```text
<!-- Code example in TEXT -->

### 3.3 RLS at Query Time

When user queries data:

```graphql
<!-- Code example in GraphQL -->
query GetPosts {
  posts {
    id
    title
  }
}
```text
<!-- Code example in TEXT -->

**Compiled to SQL with RLS:**

```sql
<!-- Code example in SQL -->
SELECT id, title
FROM v_post
WHERE
  -- RLS rule automatically added
  organization_id = $current_user_organization_id
ORDER BY created_at DESC
```text
<!-- Code example in TEXT -->

**Result:**

```json
<!-- Code example in JSON -->
{
  "data": {
    "posts": [
      {"id": "post-1", "title": "Post from my org"},
      {"id": "post-2", "title": "Another from my org"}
      // Posts from other organizations not included
    ]
  }
}
```text
<!-- Code example in TEXT -->

### 3.4 RLS Enforcement Points

RLS is enforced at multiple points:

```text
<!-- Code example in TEXT -->
Query Compilation
  ↓
Add RLS WHERE clause
  ↓
Query Execution
  ↓
Database executes filtered query
  ↓
Results returned to user
  ↓
Response transformation (field masking applied)
```text
<!-- Code example in TEXT -->

**If query attempts to bypass RLS:**

```graphql
<!-- Code example in GraphQL -->
# Malicious query: Try to get all posts
query {
  posts(where: { organization_id: "other-org" }) {
    id
  }
}

# Fails: RLS clause prevents it
SELECT id FROM v_post
WHERE
  organization_id = $current_user_organization_id  # RLS enforced
  AND organization_id = 'other-org'                 # User's filter
  # Cannot satisfy both conditions if user in different org
```text
<!-- Code example in TEXT -->

---

## 4. Field-Level Masking

### 4.1 Masking Rules

Field masking hides sensitive data from unauthorized users:

```python
<!-- Code example in Python -->
@FraiseQL.type
class User:
    id: ID
    username: str

    @FraiseQL.mask(
        show_to=["owner", "admin"],           # Roles that see real value
        hide_from=["public", "guest"],        # Roles that see masked value
        masked_value=None                     # What to show if masked
    )
    email: str

    @FraiseQL.mask(
        show_to=["owner"],                    # Only owner sees SSN
        masked_value="***-**-****"            # Show masked format
    )
    ssn: str

    @FraiseQL.mask(
        show_to=["admin"],                    # Only admin sees password
        masked_value="[REDACTED]"             # Show redacted marker
    )
    password_hash: str
```text
<!-- Code example in TEXT -->

### 4.2 Masking at Response Time

Masking is applied **after** authorization check:

```text
<!-- Code example in TEXT -->

1. Authorization check: Can user access field?
   ├─ If no → Return null error
   └─ If yes → Continue

2. Fetch field value from database

3. Check masking rule: Should field be masked?
   ├─ If no → Return real value
   └─ If yes → Return masked_value

4. Return to client
```text
<!-- Code example in TEXT -->

**Example:**

```graphql
<!-- Code example in GraphQL -->
query GetUser($id: ID!) {
  user(id: $id) {
    id
    username
    email          # @mask(show_to=["owner", "admin"])
    ssn            # @mask(show_to=["owner"])
  }
}
```text
<!-- Code example in TEXT -->

**Response for user who is NOT owner and NOT admin:**

```json
<!-- Code example in JSON -->
{
  "data": {
    "user": {
      "id": "user-123",
      "username": "alice",
      "email": null,           // Masked
      "ssn": null              // Masked
    }
  }
}
```text
<!-- Code example in TEXT -->

**Response for user who IS owner:**

```json
<!-- Code example in JSON -->
{
  "data": {
    "user": {
      "id": "user-123",
      "username": "alice",
      "email": "alice@company.com",    // Not masked
      "ssn": "***-**-****"              // Masked (only owner, not admin)
    }
  }
}
```text
<!-- Code example in TEXT -->

**Response for user who IS admin:**

```json
<!-- Code example in JSON -->
{
  "data": {
    "user": {
      "id": "user-123",
      "username": "alice",
      "email": "alice@company.com",    // Not masked
      "ssn": "123-45-6789"              // Not masked (admin can see)
    }
  }
}
```text
<!-- Code example in TEXT -->

### 4.3 Masking Strategies

Different masking strategies for different field types:

```python
<!-- Code example in Python -->
@FraiseQL.type
class Customer:
    # Strategy 1: Return null (most common)
    @FraiseQL.mask(show_to=["admin"], masked_value=None)
    credit_card: str

    # Strategy 2: Return placeholder
    @FraiseQL.mask(show_to=["owner"], masked_value="**** **** **** 1234")
    full_credit_card: str

    # Strategy 3: Return empty list
    @FraiseQL.mask(show_to=["admin"], masked_value=[])
    transaction_history: [Transaction]

    # Strategy 4: Return default value
    @FraiseQL.mask(show_to=["owner"], masked_value=0)
    balance: float

    # Strategy 5: Return random value
    @FraiseQL.mask(
        show_to=["admin"],
        masked_value_generator=lambda: random.random() * 100
    )
    approximate_balance: float
```text
<!-- Code example in TEXT -->

---

## 5. Query-Level Authorization

### 5.1 Query Authorization

Control who can execute specific queries:

```python
<!-- Code example in Python -->
@FraiseQL.query
@FraiseQL.authorize(rule="authenticated")
def get_user(id: ID!) -> User:
    """Any authenticated user can read users"""
    pass

@FraiseQL.query
@FraiseQL.authorize(rule="admin_only")
def get_all_users() -> [User!]!:
    """Only admins can list all users"""
    pass

@FraiseQL.query
@FraiseQL.authorize(rule="organization_member")
def get_organization_users(org_id: ID!) -> [User!]!:
    """Members of organization can list users in organization"""
    pass
```text
<!-- Code example in TEXT -->

### 5.2 Authorization Rules Evaluation

```python
<!-- Code example in Python -->
# Rule: "authenticated"
if not user_context.authenticated:
    raise AuthorizationError("E_AUTH_PERMISSION_401")

# Rule: "admin_only"
if "admin" not in user_context.roles:
    raise AuthorizationError("E_AUTH_PERMISSION_401")

# Rule: "organization_member"
if args.org_id not in user_context.organizations:
    raise AuthorizationError("E_AUTH_PERMISSION_401")
```text
<!-- Code example in TEXT -->

---

## 6. Built-In Authorization Rules

### 6.1 Pre-Defined Rules

FraiseQL includes built-in authorization rules:

```python
<!-- Code example in Python -->
# Authentication rules
"authenticated"          # User must be logged in
"not_authenticated"      # User must NOT be logged in

# Role-based rules
"admin_only"            # User must have 'admin' role
"user_only"             # User must have 'user' role
"moderator_only"        # User must have 'moderator' role

# Ownership rules
"owner_only"            # Current user must own resource (user_id == resource.owner_id)
"owner_or_admin"        # Current user owns resource OR is admin
"team_member"           # Current user is in resource's team

# Organization rules
"same_organization"     # Resource in same organization as user
"organization_member"   # User is member of resource's organization
"organization_admin"    # User is admin of resource's organization

# Public rules
"public"                # Anyone can access
"none"                  # No authorization (deny all)
```text
<!-- Code example in TEXT -->

### 6.2 Custom Rules

Define custom authorization rules:

```python
<!-- Code example in Python -->
@FraiseQL.authorization_rule(name="published_or_author")
def rule_published_or_author(
    resource: Any,
    user_context: UserContext
) -> bool:
    """Published posts or posts by current user"""
    return (
        resource.published
        or resource.author_id == user_context.user_id
    )

@FraiseQL.authorization_rule(name="my_department")
def rule_my_department(
    resource: Any,
    user_context: UserContext
) -> bool:
    """Resources in user's department"""
    return resource.department == user_context.metadata["department"]

# Use in schema:
@FraiseQL.type
class Post:
    @FraiseQL.authorize(rule="published_or_author")
    content: str

@FraiseQL.type
class Project:
    @FraiseQL.authorize(rule="my_department")
    budget: float
```text
<!-- Code example in TEXT -->

---

## 7. Audit Logging

### 7.1 Audit Events

Every access attempt is logged to audit trail:

```python
<!-- Code example in Python -->
class AuditEvent:
    timestamp: datetime             # When access occurred
    user_id: str                    # Who accessed
    action: str                     # "query", "mutation", "subscription"
    resource_type: str              # "Post", "User", "AdminPanel"
    resource_id: str | None         # Specific resource accessed
    operation_name: str             # "GetUserPosts", "CreatePost"
    authorization_result: bool      # Allowed or denied
    authorization_rule: str         # Which rule was evaluated
    fields_accessed: list[str]      # ["id", "title", "content"]
    fields_masked: list[str]        # ["email"] (if masked)
    rows_affected: int              # For mutations
    error_code: str | None          # If failed
    ip_address: str                 # Client IP
    user_agent: str                 # Client user agent
    trace_id: str                   # Link to request trace
```text
<!-- Code example in TEXT -->

### 7.2 Audit Log Format

```json
<!-- Code example in JSON -->
{
  "timestamp": "2026-01-15T10:30:45.123Z",
  "event_type": "query_executed",
  "user_id": "user-456",
  "action": "query",
  "resource_type": "Post",
  "resource_id": "post-789",
  "operation_name": "GetUserPosts",
  "authorization": {
    "allowed": true,
    "rule": "same_organization",
    "evaluation_time_ms": 2
  },
  "fields_accessed": ["id", "title", "author"],
  "fields_masked": [],
  "error": null,
  "request": {
    "ip_address": "203.0.113.45",
    "user_agent": "Mozilla/5.0...",
    "trace_id": "trace-abc123"
  }
}
```text
<!-- Code example in TEXT -->

**Audit log for denied access:**

```json
<!-- Code example in JSON -->
{
  "timestamp": "2026-01-15T10:30:46.456Z",
  "event_type": "access_denied",
  "user_id": "user-456",
  "action": "mutation",
  "resource_type": "Post",
  "resource_id": "post-789",
  "operation_name": "DeletePost",
  "authorization": {
    "allowed": false,
    "rule": "admin_only",
    "evaluation_time_ms": 1
  },
  "error": {
    "code": "E_AUTH_PERMISSION_401",
    "message": "User must be admin to delete posts"
  },
  "request": {
    "ip_address": "203.0.113.45",
    "user_agent": "Mozilla/5.0...",
    "trace_id": "trace-def456"
  }
}
```text
<!-- Code example in TEXT -->

### 7.3 Audit Log Persistence

Audit logs are written to:

1. **Audit log table** (`tb_audit_log`) — Queryable via SQL
2. **Immutable log stream** — Cannot be modified (append-only)
3. **External audit service** — For compliance (optional)
4. **Event bus** — For real-time processing (optional)

```sql
<!-- Code example in SQL -->
-- Audit log schema
CREATE TABLE tb_audit_log (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMP NOT NULL,
    user_id UUID NOT NULL,
    action VARCHAR NOT NULL,
    resource_type VARCHAR NOT NULL,
    resource_id UUID,
    operation_name VARCHAR NOT NULL,
    authorization_allowed BOOLEAN NOT NULL,
    authorization_rule VARCHAR,
    fields_accessed JSONB,
    error_code VARCHAR,
    ip_address INET,
    trace_id UUID,
    created_at TIMESTAMP DEFAULT NOW() NOT NULL
);

-- Index for common queries
CREATE INDEX idx_audit_user_time ON tb_audit_log(user_id, timestamp DESC);
CREATE INDEX idx_audit_resource ON tb_audit_log(resource_type, resource_id);
```text
<!-- Code example in TEXT -->

### 7.4 Audit Queries

Query audit trail for compliance:

```sql
<!-- Code example in SQL -->
-- Who accessed this sensitive record?
SELECT * FROM tb_audit_log
WHERE resource_type = 'User' AND resource_id = 'user-123'
ORDER BY timestamp DESC;

-- Did authorization ever fail for this user?
SELECT * FROM tb_audit_log
WHERE user_id = 'user-456' AND authorization_allowed = false
ORDER BY timestamp DESC;

-- What did admin do in last 24 hours?
SELECT * FROM tb_audit_log
WHERE authorization_rule = 'admin_only'
  AND timestamp > NOW() - INTERVAL 1 DAY
ORDER BY timestamp DESC;

-- When was sensitive field accessed?
SELECT * FROM tb_audit_log
WHERE fields_accessed @> '["ssn", "credit_card"]'
ORDER BY timestamp DESC;
```text
<!-- Code example in TEXT -->

---

## 8. Compliance & Security Standards

### 8.1 GDPR Compliance

FraiseQL supports GDPR requirements:

```python
<!-- Code example in Python -->
# Right to be forgotten
# User can request data deletion
@FraiseQL.mutation
@FraiseQL.authorize(rule="owner_only")
def request_data_deletion(user_id: ID!) -> Boolean:
    """Request personal data deletion"""
    # Marks user record for deletion
    # Audit log is preserved (immutable)
    pass

# Data access logging
# All data access is audited
# Users can request access log

# Data portability
# Export user data in machine-readable format
@FraiseQL.query
@FraiseQL.authorize(rule="owner_only")
def export_user_data(user_id: ID!) -> JSON:
    """Export all user data"""
    pass
```text
<!-- Code example in TEXT -->

### 8.2 HIPAA Compliance

FraiseQL supports HIPAA requirements:

```python
<!-- Code example in Python -->
# Access controls
@FraiseQL.authorize(rule="healthcare_provider")
class PatientRecord:
    """Only healthcare providers can access"""
    id: ID
    patient_id: ID

    @FraiseQL.mask(show_to=["treating_provider"])
    medical_history: str

# Encryption
# All sensitive fields encrypted at rest
# Transmitted over HTTPS/TLS

# Audit trail
# All access to protected health information (PHI) is logged
# Audit logs retained for 6+ years

# De-identification
@FraiseQL.query
def get_anonymized_statistics() -> Statistics:
    """Return de-identified statistics"""
    pass
```text
<!-- Code example in TEXT -->

### 8.3 PCI-DSS Compliance

FraiseQL supports PCI-DSS requirements:

```python
<!-- Code example in Python -->
# Never log sensitive data
# Cardholder data never appears in logs

# Field masking for cardholder data
@FraiseQL.type
class Payment:
    id: ID

    @FraiseQL.mask(show_to=["admin"], masked_value="**** **** **** 4111")
    card_number: str

# Restrict access to cardholder data
@FraiseQL.type
class PaymentMethod:
    @FraiseQL.authorize(rule="pci_authorized")
    @FraiseQL.mask(show_to=["owner", "pci_analyst"])
    card_token: str

# Tokenization
# Store tokenized references, not card data
```text
<!-- Code example in TEXT -->

---

## 9. Security Best Practices

### 9.1 Authorization Rules

**DO:**

- ✅ Always define authorization rules on sensitive types
- ✅ Use most restrictive rule that makes sense
- ✅ Include role-based checks when applicable
- ✅ Log all access attempts (audit trail)
- ✅ Review authorization rules in code review
- ✅ Test authorization with multiple user roles
- ✅ Use custom rules for complex business logic

**DON'T:**

- ❌ Rely on client-side authorization checks
- ❌ Store authorization rules in comments only
- ❌ Use overly permissive rules (avoid "public" when inappropriate)
- ❌ Hardcode user IDs (always use user context)
- ❌ Bypass authorization checks
- ❌ Trust user-provided role or organization claims

### 9.2 Field Masking

**DO:**

- ✅ Mask PII (personally identifiable information)
- ✅ Mask sensitive financial data
- ✅ Mask authentication secrets
- ✅ Test masking with unauthorized users
- ✅ Document which fields are masked and why

**DON'T:**

- ❌ Rely on masking instead of authorization
- ❌ Mask data that's already filtered by RLS
- ❌ Return misleading masked values
- ❌ Skip masking for "less important" data

### 9.3 Audit Logging

**DO:**

- ✅ Enable audit logging in production
- ✅ Retain audit logs per compliance requirements
- ✅ Monitor for suspicious access patterns
- ✅ Regularly review audit logs
- ✅ Alert on access to sensitive data

**DON'T:**

- ❌ Disable audit logging for performance
- ❌ Delete audit logs (immutable)
- ❌ Modify audit log entries
- ❌ Log sensitive data in audit trail

---

## 10. Security Configuration

### 10.1 Configuration Options

```python
<!-- Code example in Python -->
FraiseQL.security.configure({
    # Authentication
    "authentication": {
        "enabled": True,
        "required": True,
        "provider": "oauth2",  # or "jwt", "saml", "custom"
        "token_timeout_seconds": 3600,
    },

    # Authorization
    "authorization": {
        "enabled": True,
        "cache_decisions": True,  # Cache "allowed" decisions
        "cache_ttl_seconds": 300,
        "require_explicit_allow": True,  # Deny by default
    },

    # Field masking
    "masking": {
        "enabled": True,
        "log_masked_access": True,  # Log when fields are masked
    },

    # Audit logging
    "audit": {
        "enabled": True,
        "log_level": "all",  # "all", "denied_only", "errors_only"
        "retention_days": 90,
        "export_to_external": True,
        "external_service": "splunk",
    },

    # Row-level security
    "rls": {
        "enabled": True,
        "strict_mode": True,  # Deny if no RLS rule
    },
})
```text
<!-- Code example in TEXT -->

### 10.2 Environment Variables

```bash
<!-- Code example in BASH -->
# Enable/disable security features
FRAISEQL_SECURITY_ENABLED=true

# Authentication
FRAISEQL_AUTH_PROVIDER=oauth2
FRAISEQL_AUTH_ISSUER=https://auth.example.com
FRAISEQL_AUTH_AUDIENCE=FraiseQL-api

# Audit logging
FRAISEQL_AUDIT_ENABLED=true
FRAISEQL_AUDIT_RETENTION_DAYS=90
FRAISEQL_AUDIT_EXPORT_URL=https://splunk.example.com

# Security headers
FRAISEQL_SECURITY_HEADERS_ENABLED=true
```text
<!-- Code example in TEXT -->

---

## 11. Troubleshooting Security Issues

### 11.1 User Getting "Access Denied"

**Investigation steps:**

```text
<!-- Code example in TEXT -->

1. Check if user is authenticated
   → Query: SELECT authenticated_at FROM tb_user WHERE id = 'user-456'

2. Check if user has required role
   → Query: SELECT roles FROM tb_user WHERE id = 'user-456'

3. Check authorization rule
   → Rule: @authorize(rule="admin_only")
   → User roles: ["user"] (not admin)
   → Result: Denied (correct)

4. Check if user owns resource
   → Query: SELECT owner_id FROM tb_post WHERE id = 'post-789'
   → Rule: @authorize(rule="owner_only")
   → User: user-456, Owner: user-123
   → Result: Denied (correct)

5. Check audit log for denial
   → SELECT * FROM tb_audit_log
      WHERE user_id = 'user-456'
      AND authorization_allowed = false
      AND resource_id = 'post-789'
```text
<!-- Code example in TEXT -->

### 11.2 User Seeing Data They Shouldn't See

**Investigation steps:**

```text
<!-- Code example in TEXT -->

1. Check RLS rule on type
   → Query: SELECT rls_rule FROM tb_schema_types WHERE name = 'Post'

2. Check if RLS rule is applied
   → Query audit log: Is organization_id filter present?

3. Check field masking
   → Query: Is field marked with @mask?

4. Check if authorization passed
   → Did field pass authorization check?

5. Verify user's organization context
   → SELECT organization_id FROM tb_user WHERE id = 'user-456'
   → Does query filter match this org?
```text
<!-- Code example in TEXT -->

---

## 12. Summary: Security Architecture

```text
<!-- Code example in TEXT -->
┌──────────────────────────────────────────┐
│ User Request (with JWT token)            │
└────────────┬─────────────────────────────┘
             │
      ┌──────▼──────┐
      │ Authenticate│ Verify JWT, build UserContext
      │  (external) │
      └──────┬──────┘
             │
      ┌──────▼──────────────┐
      │ Query Authorization │ Type-level check: Can user execute query?
      │ (compile-time)      │
      └──────┬──────────────┘
             │
      ┌──────▼──────────────┐
      │ Field Authorization │ Field-level check: Can user read field?
      │ (compile-time)      │
      └──────┬──────────────┘
             │
      ┌──────▼──────────────┐
      │ Row-Level Security  │ Filter: Only return rows user can see
      │ (compile-time + RLS │
      └──────┬──────────────┘
             │
      ┌──────▼──────────────┐
      │ Field Masking       │ Mask: Hide sensitive fields per user
      │ (runtime)           │
      └──────┬──────────────┘
             │
      ┌──────▼──────────────┐
      │ Audit Log           │ Log: Who accessed what and when
      │ (append-only)       │
      └──────┬──────────────┘
             │
      ┌──────▼──────────────┐
      │ Response            │ Return to client
      └─────────────────────┘
```text
<!-- Code example in TEXT -->

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete specification for framework v2.x

FraiseQL's security model provides defense-in-depth through authentication, authorization, RLS, masking, and audit logging. Security is declarative; FraiseQL enforces it.
