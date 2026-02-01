# Language Authoring Alignment Strategy: TOML-Based Configuration

**Date**: February 1, 2026
**Status**: 📋 **DRAFT STRATEGY**
**Goal**: Reduce language implementation scope to essentials via TOML-based configuration

---

## Executive Summary

Instead of implementing full schema authoring in 16 languages, FraiseQL v2 should use a **TOML-based configuration approach** where:

- **Languages implement**: Type definitions, query/mutation declarations (minimal)
- **TOML implements**: Security, observers, federation, caching, all configuration
- **CLI compiles**: language output + TOML → schema.compiled.json

**Benefit**: Reduces per-language scope from 9 features to **2 features**, enabling rapid implementation across all 16 languages.

---

## Current vs. Proposed Architecture

### Current State (All-or-Nothing)
```
Language (Python/TS/Java)
├─ Type System
├─ Decorators
├─ Schema Compilation
├─ Scalar Types
├─ Queries/Mutations
├─ Federation
├─ Observers
├─ Analytics
└─ Security
  → Output: schema.json
  → Compiler: fraiseql-cli (validates + compiles)
```

**Problem**: Only 3 languages fully implemented. Others stuck at security only.

### Proposed State (TOML-Driven)
```
Language (All 16)
├─ Type Definitions (@type, class User)
└─ Query/Mutation Declarations (@query, def users())
  → Output: types.json + queries.json (minimal)

TOML File (fraiseql.toml - Single Source of Truth)
├─ [schema]
│  ├─ name
│  ├─ version
│  └─ database_target
│
├─ [types.User]
│  ├─ sql_source = "v_user"
│  └─ description
│
├─ [security]
│  ├─ [[security.rules]]
│  │  ├─ name = "isOwner"
│  │  ├─ rule = "context.user_id == resource.owner_id"
│  │  └─ cacheable = true
│  │
│  └─ [[security.policies]]
│     ├─ name = "piiAccess"
│     ├─ type = "RBAC"
│     └─ rule = "hasRole($context, 'data_manager')"
│
├─ [federation]
│  ├─ enabled = true
│  └─ [[federation.entities]]
│     ├─ name = "User"
│     └─ key_fields = ["id"]
│
├─ [observers]
│  ├─ backend = "redis"
│  └─ [[observers.handlers]]
│     ├─ name = "sendSlackNotification"
│     ├─ event = "User:Created"
│     └─ action = "slack_webhook"
│
├─ [caching]
│  ├─ enabled = true
│  └─ [[caching.rules]]
│     ├─ query = "users"
│     ├─ ttl_seconds = 300
│     └─ invalidation_triggers = ["User:Updated", "User:Deleted"]
│
└─ [database]
   ├─ url = "postgresql://localhost/mydb"
   └─ pool_size = 10

  → Output: Complete schema.compiled.json (with all config)
  → Compiler: fraiseql-cli (merges language + TOML)
```

**Benefit**: Each language only needs 2-3 modules. Configuration lives in TOML.

---

## Implementation Scope Comparison

### Per-Language Implementation

#### Before (Current - Full Scope)
| Feature | LOC | Complexity |
|---------|-----|-----------|
| Type System | 500-1000 | High |
| Decorators | 300-500 | Medium |
| Schema Compilation | 1000-2000 | High |
| Scalar Types (56) | 2000-3000 | High |
| Queries/Mutations | 300-800 | Medium |
| Federation | 1000-1500 | Very High |
| Observers | 500-1000 | Medium |
| Analytics | 300-500 | Low |
| Security | 500-1000 | Medium |
| **Total** | **6500-12000 LOC** | **Very High** |

#### After (Proposed - TOML-Driven)
| Feature | LOC | Complexity |
|---------|-----|-----------|
| Type System | 300-500 | Low |
| Decorators | 200-300 | Low |
| JSON Output | 100-200 | Very Low |
| **Total** | **600-1000 LOC** | **Low** |
| **Reduction** | **~85%** | **Huge** |

### TOML Configuration (Implemented Once)
| Feature | LOC | Owner |
|---------|-----|-------|
| Scalar Types (56) | 1000 | Core team |
| Federation | 500 | Core team |
| Observers | 800 | Core team |
| Security/RBAC | 1000 | Core team |
| Analytics | 500 | Core team |
| Caching | 400 | Core team |
| **Total** | **4200 LOC** | **Core team (once)** |

**Advantage**: Core team implements once, all 16 languages benefit immediately.

---

## Language Tier System

### Tier 1: Fully Supported (Production)
**Requirement**: Type system + query decorators + full JSON output

**Languages**:
- Python (3.10+)
- TypeScript (4.0+)
- Java (17+)

**Timeline**: Available now ✅
**Support**: Production support, SLA guaranteed

---

### Tier 2: Community Supported (Ready)
**Requirement**: Basic type definitions + JSON output (no decorators needed)

**Languages**:
- Go (1.18+)
- PHP (8.0+)
- Ruby (3.0+)
- Kotlin (1.6+)
- C# (10+)
- Rust (1.70+)

**Timeline**: 1-2 weeks per language
**Support**: Community support, best effort

**Why simple**: No decorators, just JSON generation
- Go: struct tags → JSON
- PHP: Attributes → JSON
- Ruby: Hash → JSON
- Kotlin: Data classes → JSON
- C#: Records → JSON
- Rust: Macros → JSON

---

### Tier 3: Minimal Support (Basic)
**Requirement**: JSON builders (no syntax sugar)

**Languages**:
- Node.js (18+)
- Dart (3.0+)
- Elixir (1.14+)
- Swift (5.7+)

**Timeline**: 3-5 days per language
**Support**: Documentation only

**Why simple**: Builder pattern only, no decorators
```typescript
// Node.js example
FraiseQL.defineType("User")
  .field("id", "Int")
  .field("name", "String")
  .toJSON()
```

---

### Tier 4: Future (Planned)
**Requirement**: YAML-only (no language support needed)

**Languages**:
- Scala (2.13+)
- Groovy (3.0+)
- Clojure (1.11+)

**Timeline**: Post-v2.0.0
**Support**: YAML definitions only, no language integration

---

## TOML Configuration Specification

### Full TOML Schema

```toml
# fraiseql.toml - Complete FraiseQL v2 Configuration

[schema]
# Basic project metadata
name = "myapp"
version = "1.0.0"
description = "My FraiseQL application"
database_target = "postgresql"  # postgresql, mysql, sqlite, sqlserver

[database]
# Database connection
url = "postgresql://user:pass@localhost/mydb"
pool_size = 10
ssl_mode = "prefer"
timeout_seconds = 30

# ============================================================================
# TYPE DEFINITIONS (replaces language type systems)
# ============================================================================

[types.User]
sql_source = "v_user"
description = "User entity"

  [types.User.fields]
  id = { type = "ID", nullable = false }
  name = { type = "String", nullable = false }
  email = { type = "String", nullable = true }
  created_at = { type = "DateTime", nullable = false }

[types.Post]
sql_source = "v_post"
description = "Blog post"

  [types.Post.fields]
  id = { type = "ID", nullable = false }
  title = { type = "String", nullable = false }
  author_id = { type = "ID", nullable = false }
  created_at = { type = "DateTime", nullable = false }

# ============================================================================
# QUERY & MUTATION DEFINITIONS (replaces language queries)
# ============================================================================

[queries.users]
return_type = "User"
return_array = true
sql_source = "v_user"
description = "Get all users with optional filtering"

  [[queries.users.args]]
  name = "limit"
  type = "Int"
  default = 10
  description = "Maximum number of results"

  [[queries.users.args]]
  name = "offset"
  type = "Int"
  default = 0

[queries.user]
return_type = "User"
sql_source = "v_user"
description = "Get a single user by ID"

  [[queries.user.args]]
  name = "id"
  type = "ID"
  required = true

[mutations.createUser]
return_type = "User"
sql_source = "fn_create_user"
operation = "CREATE"
description = "Create a new user"

  [[mutations.createUser.args]]
  name = "name"
  type = "String"
  required = true

  [[mutations.createUser.args]]
  name = "email"
  type = "String"
  required = true

# ============================================================================
# FEDERATION (replaces language federation modules)
# ============================================================================

[federation]
enabled = true
apollo_version = 2

  [[federation.entities]]
  name = "User"
  key_fields = ["id"]

  [[federation.entities]]
  name = "Post"
  key_fields = ["id"]

# ============================================================================
# SECURITY & AUTHORIZATION (replaces language security modules)
# ============================================================================

[security]
default_policy = "authenticated"

  # Authorization rules (expression-based)
  [[security.rules]]
  name = "isOwner"
  rule = "$context.user_id == $resource.owner_id"
  description = "User owns the resource"
  cacheable = true
  cache_ttl_seconds = 300

  [[security.rules]]
  name = "hasAdminRole"
  rule = "hasRole($context.roles, 'admin')"
  description = "User is admin"
  cacheable = true

  # Role-Based Access Control (RBAC)
  [[security.policies]]
  name = "adminOnly"
  type = "RBAC"
  rule = "hasRole($context.roles, 'admin')"
  description = "Admin access only"
  cache_ttl_seconds = 600

  [[security.policies]]
  name = "piiAccess"
  type = "RBAC"
  roles = ["data_manager", "compliance"]
  strategy = "ANY"  # ANY, ALL, EXACTLY
  description = "Access PII data"

  # Attribute-Based Access Control (ABAC)
  [[security.policies]]
  name = "clearance"
  type = "ABAC"
  attributes = [
    "clearance_level >= 3",
    "background_check == true"
  ]
  description = "Security clearance check"

  # Field-level authorization (applied per field)
  [[security.field_auth]]
  type_name = "User"
  field_name = "ssn"
  policy = "piiAccess"

  [[security.field_auth]]
  type_name = "User"
  field_name = "email"
  policy = "authenticated"

# ============================================================================
# OBSERVERS & EVENT SYSTEM (replaces language observer modules)
# ============================================================================

[observers]
enabled = true
backend = "redis"  # redis, nats, postgresql, mysql, in-memory
redis_url = "redis://localhost:6379"

  # Event handlers
  [[observers.handlers]]
  name = "slackNotification"
  event = "User:Created"
  action = "slack"
  webhook_url = "https://hooks.slack.com/services/..."
  retry_strategy = "exponential"
  max_retries = 3
  description = "Notify Slack when user created"

  [[observers.handlers]]
  name = "emailConfirmation"
  event = "User:Created"
  action = "email"
  template = "welcome_email"
  retry_strategy = "linear"
  max_retries = 5

  [[observers.handlers]]
  name = "auditLog"
  event = "*"  # All events
  action = "audit_log"
  description = "Log all changes for compliance"

# ============================================================================
# CACHING (replaces language caching modules)
# ============================================================================

[caching]
enabled = true
backend = "redis"  # redis, memory, postgresql
redis_url = "redis://localhost:6379"

  [[caching.rules]]
  query = "users"
  ttl_seconds = 300
  invalidation_triggers = ["User:Created", "User:Updated", "User:Deleted"]

  [[caching.rules]]
  query = "user"
  ttl_seconds = 3600
  invalidation_triggers = ["User:Updated", "User:Deleted"]

  [[caching.rules]]
  query = "posts"
  ttl_seconds = 600
  invalidation_triggers = ["Post:Created", "Post:Updated", "Post:Deleted"]

# ============================================================================
# ENTERPRISE SECURITY (replaces language security extensions)
# ============================================================================

[security.enterprise]
# Rate limiting
rate_limiting_enabled = true
auth_endpoint_max_requests = 100
auth_endpoint_window_seconds = 60

# Audit logging
audit_logging_enabled = true
audit_log_backend = "postgresql"  # postgresql, elasticsearch
audit_retention_days = 365

# Error sanitization
error_sanitization = true
hide_implementation_details = true

# Timing attack prevention
constant_time_comparison = true

# PKCE for OAuth
pkce_enabled = true

# ============================================================================
# ANALYTICS (replaces language analytics modules)
# ============================================================================

[analytics]
enabled = true

  [[analytics.queries]]
  name = "usersByCountry"
  sql_source = "tf_users_by_country"  # Fact table
  description = "Users grouped by country"

  [[analytics.queries]]
  name = "revenue"
  sql_source = "tf_revenue"
  description = "Revenue metrics"

# ============================================================================
# MONITORING & OBSERVABILITY
# ============================================================================

[observability]
# Prometheus metrics
prometheus_enabled = true
prometheus_port = 9090

# OpenTelemetry tracing
otel_enabled = true
otel_exporter = "jaeger"  # jaeger, datadog, otlp
otel_jaeger_endpoint = "http://localhost:14250"

# Health checks
health_check_enabled = true
health_check_interval_seconds = 30

# Logging
log_level = "info"  # debug, info, warn, error
log_format = "json"  # json, text
```

---

## Language Implementation Scope (TOML-Based)

### What Each Language Implements

#### Tier 1 (Python, TypeScript, Java)
```python
# Python example
from fraiseql import type, query

@type
class User:
    id: int
    name: str
    email: str | None

@query(sql_source="v_user")
def users(limit: int = 10) -> list[User]:
    pass

# Export generates types.json
fraiseql.export_schema("types.json")
```

**Language does**:
- ✅ Type definitions with field metadata
- ✅ Query/mutation declarations with args
- ✅ Generate types.json (types + queries only)

**TOML provides**:
- ✅ Type bindings (sql_source, description)
- ✅ Query return types, descriptions, args metadata
- ✅ All configuration (security, federation, etc.)

---

#### Tier 2 (Go, PHP, Ruby, Kotlin, C#, Rust)
```typescript
// Go example
type User struct {
  ID    string `fraiseql:"id,type=ID"`
  Name  string `fraiseql:"name,type=String"`
  Email string `fraiseql:"email,type=String,nullable"`
}

// Export as JSON
fraiseql.ExportSchema("types.json")
```

**Language does**:
- ✅ Struct/class definitions with tags/attributes
- ✅ Generate JSON representation

**TOML provides**:
- ✅ All metadata and configuration

---

#### Tier 3 (Node.js, Dart, Elixir, Swift)
```typescript
// Node.js example
const User = FraiseQL.type("User")
  .field("id", "ID")
  .field("name", "String")
  .field("email", "String", { nullable: true })
  .toJSON();
```

**Language does**:
- ✅ Builder API for types
- ✅ JSON output

**TOML provides**:
- ✅ All metadata and configuration

---

## Migration Path from Current to TOML-Based

### Phase 1: Deprecation (v2.0.0 alpha to beta)
- Current implementations continue to work
- Add deprecation warnings if using language security modules
- Recommend TOML approach for new projects

### Phase 2: TOML Support (v2.0.0 beta to GA)
- Implement fraiseql-cli merger: language output + TOML → compiled schema
- Document TOML configuration
- Create TOML examples for all use cases

### Phase 3: Migration (v2.1.0)
- Default to TOML
- Language modules simplified to stubs
- Encourage migration via guides

### Phase 4: Consolidation (v2.2.0+)
- Remove complex features from language modules
- Tier 3/4 languages require TOML only
- Language modules become optional

---

## Implementation Steps

### Step 1: TOML Schema Design (1-2 days)
- ✅ **DONE** - See above TOML specification
- All configuration options documented
- Examples for each feature

### Step 2: CLI Enhancement (3-5 days)
```bash
# Current workflow
python schema.py → schema.json
fraiseql-cli compile schema.json → schema.compiled.json

# New workflow
python schema.py → types.json
fraiseql-cli compile \
  --types types.json \
  --config fraiseql.toml \
  --output schema.compiled.json

# Or combined
fraiseql-cli compile fraiseql.toml --output schema.compiled.json
```

**Changes**:
- Parse TOML configuration
- Merge types.json + TOML
- Validate combined schema
- Generate schema.compiled.json

### Step 3: Tier 1 Languages (3-5 days each)
Update Python, TypeScript, Java to:
- Generate minimal types.json (types + queries only)
- Remove federation/security code
- Keep decorators/builders for DX

### Step 4: Tier 2 Languages (2-3 days each)
Implement Go, PHP, Ruby, Kotlin, C#, Rust:
- Struct/class tags → types.json
- JSON builder pattern
- Basic type system only

### Step 5: Tier 3 Languages (1-2 days each)
Implement Node.js, Dart, Elixir, Swift:
- Simple builder API
- JSON output
- No decorators

### Step 6: Documentation (2-3 days)
- TOML configuration guide
- Tier system explanation
- Migration guide from current to TOML
- Examples per language

---

## Benefits of TOML-Based Approach

### For Users
- ✅ Single source of truth (fraiseql.toml)
- ✅ Language-agnostic configuration
- ✅ Easy to modify without recompiling language
- ✅ No vendor lock-in to specific language
- ✅ Clear organization of all settings

### For Maintainers
- ✅ 85% reduction in per-language scope
- ✅ All 16 languages can reach Tier 2+ quickly
- ✅ Core team implements features once (in TOML parser)
- ✅ No need to implement federation in 16 languages
- ✅ Easier to add new features (just extend TOML)

### For Community
- ✅ Can implement language in 600-1000 LOC vs 6500-12000
- ✅ Tier 3/4 languages become feasible
- ✅ Language generators become simple, maintainable
- ✅ Clear contribution guidelines

---

## Timeline & Effort

| Phase | Component | Effort | Timeline |
|-------|-----------|--------|----------|
| Design | TOML schema | 1 day | Week 1 |
| Implementation | CLI enhancement | 3-5 days | Week 1-2 |
| Tier 1 | Python/TS/Java updates | 5 days | Week 2 |
| Tier 2 | Go/PHP/Ruby/Kotlin/C#/Rust | 15 days | Week 3-4 |
| Tier 3 | Node.js/Dart/Elixir/Swift | 8 days | Week 4 |
| Tier 4 | Scala/Groovy/Clojure | 5 days | Week 5 |
| Docs | TOML guide + examples | 3 days | Week 5 |
| **TOTAL** | **All 16 languages + TOML** | **~40 days** | **~6 weeks** |

**vs Current**: Getting Node.js/Ruby/etc. to feature parity would take 60+ days

---

## Example: Migration of Go Language

### Before (Full Implementation)
```go
// decorators.go - 500+ LOC
// types.go - 1000+ LOC
// schema.go - 1000+ LOC
// federation.go - 500+ LOC
// observers.go - 500+ LOC
// security.go - 500+ LOC
// Total: 4000+ LOC
```

### After (TOML-Based)
```go
// types.go - 300 LOC
package fraiseql

type User struct {
  ID    string `fraiseql:"id,type=ID"`
  Name  string `fraiseql:"name,type=String"`
  Email string `fraiseql:"email,type=String,nullable"`
}

// Export as JSON (100 LOC)
func ExportSchema(filename string) error {
  types := []Type{
    {Name: "User", Fields: extractFields(User{})},
  }
  return writeJSON(filename, types)
}
```

**Reduction**: 4000 LOC → 400 LOC (**90% smaller**)

**fraiseql.toml** provides everything else:
```toml
[types.User]
sql_source = "v_user"

[queries.users]
return_type = "User"
sql_source = "v_user"

[security.policies.adminOnly]
type = "RBAC"
rule = "hasRole($context.roles, 'admin')"

[federation]
enabled = true
```

---

## Recommended Decision

### Option A: TOML-First (Recommended)
- Implement TOML support in CLI
- Update Tier 1 languages to use TOML
- Implement Tier 2/3/4 languages with TOML
- Deprecate complex language modules
- **Timeline**: 6 weeks, all 16 languages at Tier 2+

### Option B: Hybrid (Current + TOML)
- Keep current Tier 1 implementations
- Add TOML support as alternative
- Implement Tier 2/3/4 with TOML
- Maintain both approaches
- **Timeline**: 8 weeks, 3 languages Tier 1, rest Tier 2+

### Option C: Status Quo (No Change)
- Keep current implementation
- Only Tier 1 (3 languages) works
- Others stuck at security-only
- **Timeline**: Indefinite, 13 languages incomplete

---

## Recommendation

**Proceed with Option A (TOML-First)** because:

1. ✅ Dramatically reduces implementation scope per language
2. ✅ Enables all 16 languages to reach production-ready status
3. ✅ Single source of truth for configuration
4. ✅ Easier to add features in future
5. ✅ Better user experience (TOML > decorators for config)
6. ✅ More maintainable long-term

---

## Next Steps

1. **Review TOML specification** - Confirm all features covered
2. **Design CLI merger logic** - How types.json + TOML → compiled schema
3. **Create TOML parser** - In fraiseql-cli (Rust)
4. **Update Tier 1 languages** - Point to TOML for config
5. **Implement Tier 2** - Go, PHP, Ruby, Kotlin, C#, Rust
6. **Implement Tier 3** - Node.js, Dart, Elixir, Swift
7. **Document & Release** - Complete guide + examples

---

**This approach transforms FraiseQL from 3 fully-supported languages to 16 with minimal per-language overhead.**
