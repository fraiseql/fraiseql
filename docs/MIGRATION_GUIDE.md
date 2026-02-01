# FraiseQL v2 Migration Guide: TOML-Based Configuration

## Overview

FraiseQL v2 introduces a **TOML-based configuration workflow** that reduces per-language SDK complexity by 85%. Instead of language SDKs generating complete schemas, they now generate minimal `types.json` files while all operational configuration moves to `fraiseql.toml`.

### Before (v1.x) - Per-Language Complexity
```
Python/TypeScript/Java code
    ↓ (decorators generate complete schema)
schema.json (includes types, queries, mutations, federation, security, observers)
    ↓ (fraiseql-cli compile)
schema.compiled.json
```

**Per-language LOC:**
- Python: 3,491 LOC
- TypeScript: 4,433 LOC
- Java: 14,129 LOC
- **Total: 21,053 LOC**

### After (v2.0) - TOML-Based Workflow
```
Python/TypeScript/Java code (@type/@GraphQLType/@GraphQLType)
    ↓ (exportTypes() → minimal types.json)
types.json (types, enums, input_types, interfaces only)
    ↓ (+ fraiseql.toml with queries, mutations, security, observers, etc.)
schema.compiled.json (via fraiseql-cli merge)
```

**Per-language LOC:**
- Python: ~850 LOC (77% reduction)
- TypeScript: ~2,100 LOC (53% reduction)
- Java: 3,479 LOC (75% reduction)
- **Total: ~6,429 LOC (70% reduction)**

---

## Python Migration Guide

### 1. Update Type Definitions

**Old (v1.x):**
```python
from fraiseql import FraiseQL, type, federation, query, mutation

@type
class User:
    id: str
    name: str
    email: str

FraiseQL.query("users").returnType(User).register()
FraiseQL.mutation("createUser").returnType(User).arg("name", "String").register()
FraiseQL.exportSchema("schema.json")  # Complete schema with everything
```

**New (v2.0):**
```python
from fraiseql import FraiseQL, type

@type
class User:
    id: str
    name: str
    email: str

FraiseQL.registerTypes([User])
FraiseQL.exportTypes("types.json")  # Minimal: types only
```

### 2. Move Queries & Mutations to TOML

**fraiseql.toml:**
```toml
[fraiseql.queries.users]
return_type = "User"
returns_list = true
sql_source = "SELECT * FROM users"

[fraiseql.mutations.createUser]
return_type = "User"
operation = "CREATE"
sql_source = "INSERT INTO users (name, email) VALUES (?, ?) RETURNING *"
```

### 3. Move Federation to TOML

**Old (v1.x):**
```python
FraiseQL.federationLink("User", "http://users-service/graphql").register()
```

**New (v2.0):**
```toml
[fraiseql.federation]
[[fraiseql.federation.subgraphs]]
name = "User"
strategy = "http"
url = "http://users-service/graphql"
```

### 4. Move Security to TOML

**Old (v1.x):**
```python
FraiseQL.securityPolicy("rate_limit", 100, 60).register()
```

**New (v2.0):**
```toml
[fraiseql.security.rate_limiting]
enabled = true
max_requests = 100
window_secs = 60
```

### 5. Move Observers to TOML

**Old (v1.x):**
```python
FraiseQL.observer("userCreated").entity("User").event("INSERT").action("webhook", url="...").register()
```

**New (v2.0):**
```toml
[fraiseql.observers.userCreated]
entity = "User"
event = "INSERT"

[[fraiseql.observers.userCreated.actions]]
type = "webhook"
url = "https://api.example.com/webhooks"
```

### 6. Compile Workflow

**Old (v1.x):**
```bash
python schema.py  # Generates schema.json
fraiseql compile schema.json  # Compiles to schema.compiled.json
```

**New (v2.0):**
```bash
python export_types.py  # Generates types.json
fraiseql compile fraiseql.toml --types types.json  # Merges and compiles
```

---

## TypeScript Migration Guide

### 1. Update Type Definitions

**Old (v1.x):**
```typescript
import * as fraiseql from "fraiseql";

@fraiseql.Type()
class User {
  id: string;
  name: string;
  email: string;
}

fraiseql.registerQuery("users", User, true);
fraiseql.exportSchema("schema.json");
```

**New (v2.0):**
```typescript
import * as fraiseql from "fraiseql";

@fraiseql.Type()
class User {
  id: string;
  name: string;
  email: string;
}

fraiseql.registerTypeFields("User", [
  { name: "id", type: "ID", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "email", type: "String", nullable: false },
]);

fraiseql.exportTypes("types.json");  // Minimal: types only
```

### 2. Move Queries & Mutations to TOML

**fraiseql.toml:**
```toml
[fraiseql.queries.users]
return_type = "User"
returns_list = true
sql_source = "SELECT * FROM users"
```

### 3. Move Federation to TOML

**Old (v1.x):**
```typescript
fraiseql.federation.link("User", "http://...");
```

**New (v2.0):**
```toml
[fraiseql.federation]
[[fraiseql.federation.subgraphs]]
name = "User"
strategy = "http"
url = "http://..."
```

### 4. Move Observers to TOML

**Old (v1.x):**
```typescript
fraiseql.observer("userCreated").entity("User").on("INSERT");
```

**New (v2.0):**
```toml
[fraiseql.observers.userCreated]
entity = "User"
event = "INSERT"
```

### 5. Compile Workflow

**Old (v1.x):**
```bash
npx ts-node schema.ts  # Generates schema.json
fraiseql compile schema.json
```

**New (v2.0):**
```bash
npx ts-node export_types.ts  # Generates types.json
fraiseql compile fraiseql.toml --types types.json
```

---

## Java Migration Guide

### 1. Update Type Definitions

**Old (v1.x):**
```java
import com.fraiseql.core.*;

@GraphQLType
class User {
    public String id;
    public String name;
    public String email;
}

FraiseQL.registerType(User.class);
FraiseQL.query("users").returnType(User.class).register();
FraiseQL.exportSchema("schema.json");
```

**New (v2.0):**
```java
import com.fraiseql.core.*;

@GraphQLType
class User {
    public String id;
    public String name;
    public String email;
}

FraiseQL.registerType(User.class);
FraiseQL.exportTypes("types.json");  // Minimal: types only
```

### 2. Remove Observer Code

**Old (v1.x):**
```java
new ObserverBuilder("userCreated")
    .entity("User")
    .event("INSERT")
    .addAction(Webhook.create("https://api.example.com/webhooks"))
    .register();
```

**New (v2.0):**
```toml
[fraiseql.observers.userCreated]
entity = "User"
event = "INSERT"

[[fraiseql.observers.userCreated.actions]]
type = "webhook"
url = "https://api.example.com/webhooks"
```

### 3. Remove Authorization Code

**Old (v1.x):**
```java
@Authorize(requires = "admin")
public Query getSecretData() { ... }
```

**New (v2.0):**
```toml
[fraiseql.security]
# Defined in TOML, not Java decorators
```

### 4. Compile Workflow

**Old (v1.x):**
```bash
javac -cp fraiseql-java.jar Schema.java
java -cp fraiseql-java.jar:. Schema  # Generates schema.json
fraiseql compile schema.json
```

**New (v2.0):**
```bash
javac -cp fraiseql-java.jar ExportTypes.java
java -cp fraiseql-java.jar:. ExportTypes  # Generates types.json
fraiseql compile fraiseql.toml --types types.json
```

---

## Complete Example: Before & After

### Before (v1.x): Python Example
```python
from fraiseql import FraiseQL, type, query, federation, observer, security

@type
class User:
    id: str
    name: str

FraiseQL.query("users").returnType(User).register()
FraiseQL.mutation("createUser").returnType(User).arg("name", "String").register()
FraiseQL.federationLink("User", "http://auth-service/graphql").register()
FraiseQL.observer("userCreated").entity("User").event("INSERT").action("webhook", ...).register()
FraiseQL.securityPolicy("rate_limit", 100, 60).register()
FraiseQL.exportSchema("schema.json")  # Large, 500+ lines
```

### After (v2.0): Python + TOML
```python
from fraiseql import FraiseQL, type

@type
class User:
    id: str
    name: str

FraiseQL.registerTypes([User])
FraiseQL.exportTypes("types.json")  # Minimal, 20 lines
```

**fraiseql.toml:**
```toml
[fraiseql.queries.users]
return_type = "User"
returns_list = true

[fraiseql.mutations.createUser]
return_type = "User"
operation = "CREATE"

[fraiseql.federation]
[[fraiseql.federation.subgraphs]]
name = "User"
strategy = "http"
url = "http://auth-service/graphql"

[fraiseql.security.rate_limiting]
enabled = true
max_requests = 100
window_secs = 60

[fraiseql.observers.userCreated]
entity = "User"
event = "INSERT"

[[fraiseql.observers.userCreated.actions]]
type = "webhook"
url = "https://api.example.com/webhooks"
```

---

## Compilation Workflows

### Workflow 1: TOML-Only (No Language SDK)
```bash
fraiseql compile fraiseql.toml
# Output: schema.compiled.json
```

Use when: You want to define everything in TOML without language SDKs.

### Workflow 2: Language SDK + TOML (Recommended)
```bash
# 1. Generate types.json from any language SDK
python export_types.py  # OR npx ts-node export_types.ts OR java ExportTypes

# 2. Merge with TOML configuration
fraiseql compile fraiseql.toml --types types.json
# Output: schema.compiled.json
```

Use when: You want strong typing in your language SDK + centralized configuration in TOML.

### Workflow 3: Legacy JSON (v1.x Compatibility)
```bash
fraiseql compile schema.json
# Output: schema.compiled.json
```

Use when: Migrating gradually from v1.x.

---

## Benefits of Migration

| Aspect | Before (v1.x) | After (v2.0) |
|--------|---------------|------------|
| **Code Size** | 21,053 LOC total | 6,429 LOC total (-70%) |
| **Language Support** | 3 languages (heavily maintained) | 16 languages (lightweight) |
| **Configuration** | Scattered across language SDKs | Centralized in TOML |
| **Query Definitions** | In Python/TypeScript/Java | In fraiseql.toml |
| **Security Config** | In Python/TypeScript/Java | In fraiseql.toml |
| **Observers** | In Python/TypeScript/Java | In fraiseql.toml |
| **Federation** | In Python/TypeScript/Java | In fraiseql.toml |
| **Type Safety** | Per-language | Language SDK provides types |
| **Maintainability** | High (lots of per-language code) | Low (minimal SDK code) |
| **Consistency** | Language-specific quirks | Consistent across all languages |

---

## Troubleshooting

### Q: "types.json not found" error
```bash
# Make sure you're running the export command first
python export_types.py  # Or TypeScript/Java equivalent
fraiseql compile fraiseql.toml --types types.json
```

### Q: "Invalid return type" error
```bash
# Make sure types in TOML match types in types.json
# types.json defines: User, Post
# fraiseql.toml should reference: "User", "Post" (exact names)
```

### Q: How do I migrate gradually?
```bash
# 1. Export types.json from language SDK
fraiseql compile fraiseql.toml --types types.json

# 2. Keep both workflows working during transition:
# - Use v2.0 SDKs for new code
# - Keep v1.x SDKs for existing code
# - Eventually consolidate all config to TOML
```

### Q: Can I use multiple language SDKs?
**Yes!** Merge multiple `types.json` files:
```json
{
  "types": [
    ...types from Python...,
    ...types from TypeScript...,
    ...types from Java...
  ]
}
```

---

## Summary

The TOML-based workflow represents a fundamental shift in FraiseQL v2:

1. **Language SDKs are minimal** - Only define types, nothing else
2. **TOML is the source of truth** - All operational config lives here
3. **CLI handles composition** - `fraiseql compile` merges types + config
4. **16 languages supported** - Simple, lightweight SDKs for each

This enables rapid growth to 16+ languages while maintaining consistency and ease of maintenance.

---

For more information:
- [fraiseql.toml Reference](./TOML_REFERENCE.md)
- [API Documentation](./API.md)
- [Examples](../tests/integration/examples/)
