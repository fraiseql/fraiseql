# Security Architecture

FraiseQL's security model is fundamentally different from traditional GraphQL frameworks. Instead of relying on runtime permissions and complex authorization middleware, FraiseQL uses PostgreSQL views to define exactly what data can be accessed at the database level.

## JSONB Views: Structural Security

### What Gets Exposed is Explicitly Defined

Every GraphQL type in FraiseQL maps to a PostgreSQL view that contains only the fields you explicitly choose to expose. This creates a two-layer verification system:

1. **Database Layer**: The view defines what data exists
2. **Application Layer**: Python types enforce the schema structure

```sql
-- Example: User view with explicit field whitelisting
CREATE VIEW user_public AS
SELECT
    id,
    email,
    -- Explicitly exclude: password_hash, ssn, internal_notes
    created_at,
    updated_at
FROM users
WHERE deleted_at IS NULL;
```

```python
# Python type matches the view exactly
class User(BaseModel):
    id: int
    email: str
    created_at: datetime
    updated_at: datetime
    # No password_hash or sensitive fields possible
```

### Impossible to Accidentally Leak Sensitive Data

Unlike ORM-based approaches where you might accidentally include sensitive fields in your GraphQL schema, FraiseQL makes it structurally impossible:

**❌ Traditional ORM Approach:**
```python
# Accidentally exposes everything from the User model
class UserType(DjangoObjectType):
    class Meta:
        model = User
        # Forgot to exclude sensitive fields!
```

**✅ FraiseQL Approach:**
```sql
-- View only contains safe fields by design
CREATE VIEW user_safe AS
SELECT id, email, created_at FROM users;
```
```python
# Type can only reference fields that exist in the view
class User(BaseModel):
    id: int
    email: str
    created_at: datetime
    # Compiler error if you try to add password_hash
```

## No ORM Over-Fetching Risks

Traditional GraphQL resolvers often over-fetch data from the database, then filter it in application code. This creates security risks where sensitive data might be temporarily loaded into memory before being filtered out.

**❌ ORM Over-Fetching Risk:**
```python
def resolve_user(self, info, user_id):
    user = User.objects.get(id=user_id)  # Loads ALL fields
    # Then filter sensitive data in Python
    return {
        'id': user.id,
        'email': user.email,
        # Hope we didn't forget to exclude user.password_hash
    }
```

**✅ FraiseQL Security by Design:**
```sql
-- Database never loads sensitive fields
CREATE VIEW user_public AS
SELECT id, email, created_at FROM users;
```

The database view ensures sensitive fields are never loaded from disk, eliminating the possibility of accidental exposure through coding errors or misconfigurations.

## What's Not in the View Cannot Be Queried

FraiseQL's security model is based on the principle that **if a field isn't in the database view, it cannot be queried**. This creates a simple, auditable security boundary:

```graphql
# This query works - fields exist in view
query {
  user(id: 1) {
    id
    email
    created_at
  }
}

# This query fails at compile time - password not in view
query {
  user(id: 1) {
    id
    password  # ❌ Field doesn't exist in database view
  }
}
```

## Two-Layer Verification

FraiseQL implements defense in depth with complementary security layers:

### Layer 1: Database View Constraints
- Defines the absolute maximum data accessible
- Enforced by PostgreSQL's view system
- Cannot be bypassed by application code

### Layer 2: Python Type System
- Provides compile-time guarantees
- IDE support for catching field access errors
- Runtime validation of data structure

## Best Practices for Secure Views

### 1. Principle of Least Privilege
```sql
-- Only expose what's needed for the specific use case
CREATE VIEW user_profile AS
SELECT
    id,
    display_name,
    avatar_url
    -- No email, no internal fields
FROM users;
```

### 2. Contextual Views for Different Roles
```sql
-- Public profile view
CREATE VIEW user_public AS
SELECT id, display_name FROM users;

-- Admin view with more fields
CREATE VIEW user_admin AS
SELECT id, email, role, last_login FROM users;

-- Self view for account management
CREATE VIEW user_self AS
SELECT id, email, display_name, settings FROM users;
```

### 3. Row-Level Security Integration
```sql
-- Combine with PostgreSQL RLS
CREATE VIEW posts_visible AS
SELECT * FROM posts
WHERE author_id = current_user_id()
   OR visibility = 'public';

ALTER VIEW posts_visible SET (security_barrier = true);
```

### 4. Audit Trail Views
```sql
-- Separate view for audit data
CREATE VIEW user_audit AS
SELECT
    id,
    created_at,
    updated_at,
    updated_by
FROM users;
```

## Migration from Traditional GraphQL

When migrating from traditional GraphQL frameworks, focus on translating your authorization logic into view definitions:

**Before:**
```python
# Complex permission checks in resolvers
def resolve_user_email(self, info):
    if not info.context.user.can_view_emails:
        return None
    return user.email
```

**After:**
```sql
-- Permission logic becomes view logic
CREATE VIEW user_with_email AS
SELECT u.id, u.email
FROM users u
JOIN user_permissions p ON p.user_id = u.id
WHERE p.can_view_emails = true;
```

This approach eliminates entire classes of security vulnerabilities by making sensitive data access impossible rather than just discouraged.
