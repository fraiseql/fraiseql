# Language Implementation Plan: TOML-Based Alignment

**Date**: February 1, 2026
**Scope**: Detailed implementation steps for each language tier
**Goal**: Complete 16-language support with TOML-driven configuration

---

## Overview

This plan details how to implement each language tier (1-4) with TOML-based configuration.

### Core Principle
Languages define **types and queries only**. Everything else (security, federation, caching, observers) comes from `fraiseql.toml`.

---

## Tier 1: Full Support (Python, TypeScript, Java)

These languages already have full implementations. **Update them** to reduce scope and use TOML for configuration.

### Python Implementation

**Current State**: 8,111 LOC, 44 tests
**Target State**: 3,000 LOC (keep decorators for DX)
**Timeline**: 2-3 days

#### Step 1: Remove Configuration Code
```python
# DELETE these modules entirely
# - federation.py (move to TOML)
# - observers.py (move to TOML)
# - analytics.py (move to TOML)
# - security.py (move to TOML [except rules])

# KEEP AND REFACTOR
# - decorators.py (minimal)
# - types.py (minimal)
# - schema.py (update to types.json only)
# - scalars.py (KEEP - used for type validation)
```

#### Step 2: Simplify Decorators
```python
# Before: decorators could take security, federation, etc.
@fraiseql.type(
    federation=True,
    auth_policy="authenticated"
)
class User:
    pass

# After: decorators for structure only
@fraiseql.type
class User:
    id: int
    name: str
```

#### Step 3: Update Schema Export
```python
# Before: generated complete schema.json
def export_schema(filename):
    schema = {
        "types": types,
        "queries": queries,
        "mutations": mutations,
        "federation": federation_config,  # REMOVE
        "security": security_config,  # REMOVE
        "observers": observer_config  # REMOVE
    }

# After: only types and queries
def export_schema(filename):
    schema = {
        "types": types,
        "queries": queries,
        "mutations": mutations
        # All config now in fraiseql.toml
    }
```

#### Step 4: Update Tests
```python
# Remove tests for:
# - Federation configuration
# - Security configuration
# - Observer configuration

# Keep tests for:
# - Type definition
# - Query definition
# - Type validation
# - Scalar type usage
```

#### Step 5: Updated README
```markdown
# FraiseQL v2 - Python Schema Authoring

Define types and queries in Python decorators.
Configuration (security, federation, caching) goes in `fraiseql.toml`.

## Installation

```bash
pip install fraiseql
```

## Quick Start

### 1. Define Types

```python
from fraiseql import type

@type
class User:
    id: int
    name: str
    email: str | None
    created_at: str
```

### 2. Define Queries

```python
from fraiseql import query

@query(sql_source="v_user")
def users(limit: int = 10) -> list[User]:
    pass

@query(sql_source="v_user")
def user(id: int) -> User | None:
    pass
```

### 3. Define Mutations

```python
from fraiseql import mutation

@mutation(sql_source="fn_create_user")
def create_user(name: str, email: str) -> User:
    pass
```

### 4. Export Schema

```python
from fraiseql import export_schema

if __name__ == "__main__":
    export_schema("types.json")
```

### 5. Create fraiseql.toml

Create a `fraiseql.toml` file with type bindings and all configuration:

```toml
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"

[queries.users]
return_type = "User"
return_array = true
sql_source = "v_user"

[queries.user]
return_type = "User"
sql_source = "v_user"

[security.policies.authenticated]
type = "RBAC"
rule = "hasRole($context.roles, 'user')"
```

### 6. Compile

```bash
fraiseql-cli compile fraiseql.toml --output schema.compiled.json
```

## Configuration

All configuration now lives in `fraiseql.toml`:
- Security (RBAC, ABAC, custom rules)
- Federation (entities, relationships)
- Observers (event handlers)
- Caching (TTL, invalidation)
- Database (connection settings)

See `fraiseql.toml` documentation for full specification.

## Advanced: Custom Scalars

```python
from fraiseql import scalar

@scalar
class Email(str):
    pattern = r"^[^@]+@[^@]+\.[^@]+$"
    description = "Valid email address"
```
```

### Effort Estimate: 2-3 days
- Remove 5000 LOC of config code
- Simplify decorators
- Update tests and docs
- Verify exports correctly

---

### TypeScript Implementation

**Current State**: 20,364 LOC, 9 tests
**Target State**: 5,000 LOC (most complete language)
**Timeline**: 3 days

#### Key Changes
1. Remove federation/observers/security/analytics modules
2. Update decorators to be type-definition only
3. Simplify schema export to types.json
4. Keep rich type system for good DX

#### Updated README
Same pattern as Python - types + queries in code, everything else in TOML.

### Effort Estimate: 3 days

---

### Java Implementation

**Current State**: 14,129 LOC, 35 tests
**Target State**: 4,000 LOC
**Timeline**: 2-3 days

#### Key Changes
1. Remove federation/security/observer builders
2. Keep annotation-based type definitions
3. Simplify configuration
4. Update test suite

#### Effort Estimate: 2-3 days

---

## Tier 2: Community Support (Go, PHP, Ruby, Kotlin, C#, Rust)

These languages need minimal implementations - just type definitions + JSON output.

### Go Implementation

**Current State**: 3,728 LOC, 7 tests (has most modules, missing federation)
**Target State**: 800 LOC (only types.json generation)
**Timeline**: 2 days

#### Step 1: Simplify to Struct Tags

```go
package myapp

import "github.com/fraiseql/fraiseql-go/v2"

type User struct {
  ID        string `fraiseql:"id,type=ID"`
  Name      string `fraiseql:"name,type=String"`
  Email     string `fraiseql:"email,type=String,nullable"`
  CreatedAt string `fraiseql:"created_at,type=DateTime"`
}

type Query struct {
  Users  []User `fraiseql:"query,sql_source=v_user"`
  User   User   `fraiseql:"query,sql_source=v_user"`
}

type Mutation struct {
  CreateUser User `fraiseql:"mutation,sql_source=fn_create_user"`
}
```

#### Step 2: Generate types.json

```go
package fraiseql

import "encoding/json"

func ExportSchema(filename string, types ...interface{}) error {
  // Reflect on struct tags
  // Extract type definitions
  // Generate types.json
  return writeJSON(filename, schema)
}

// Usage
func main() {
  fraiseql.ExportSchema("types.json", myapp.User{}, myapp.Query{}, myapp.Mutation{})
}
```

#### Step 3: Minimal Test Suite

```go
func TestUserStruct(t *testing.T) {
  // Verify tag parsing
  // Verify JSON generation
}

func TestQueryStruct(t *testing.T) {
  // Verify query definition
}
```

#### Updated README

```markdown
# FraiseQL v2 - Go Schema Authoring

Define types and queries using Go struct tags.
Configuration goes in `fraiseql.toml`.

## Installation

```bash
go get github.com/fraiseql/fraiseql-go/v2
```

## Quick Start

### 1. Define Types

```go
package myapp

type User struct {
  ID    string `fraiseql:"id,type=ID"`
  Name  string `fraiseql:"name,type=String"`
  Email string `fraiseql:"email,type=String,nullable"`
}
```

### 2. Define Queries

```go
type Queries struct {
  Users []User `fraiseql:"query,sql_source=v_user,description=Get all users"`
}
```

### 3. Export Schema

```go
func main() {
  fraiseql.ExportSchema("types.json", User{}, Queries{})
}
```

### 4. Create fraiseql.toml

```toml
[schema]
name = "myapp"
database_target = "postgresql"

[types.User]
sql_source = "v_user"

[queries.users]
return_type = "User"
return_array = true
sql_source = "v_user"
```

### 5. Compile

```bash
fraiseql-cli compile fraiseql.toml
```
```

#### Effort Estimate: 2 days
- Simplify tag system
- Write JSON exporter
- Create minimal tests
- Update docs

---

### PHP Implementation

**Current State**: 9,920 LOC, 18 tests
**Target State**: 1,500 LOC
**Timeline**: 2-3 days

#### Structure

```php
<?php
namespace FraiseQL;

#[Type]
class User {
    #[Field(type: "ID")]
    public string $id;

    #[Field(type: "String")]
    public string $name;

    #[Field(type: "String", nullable: true)]
    public ?string $email;
}

class Queries {
    #[Query(sql_source: "v_user")]
    public static function users(int $limit = 10): array {}
}

// Export
FraiseQL::export("types.json", User::class, Queries::class);
```

#### Effort Estimate: 2-3 days

---

### Ruby Implementation

**Current State**: 1,386 LOC, 7 tests (currently only security)
**Target State**: 600 LOC
**Timeline**: 1-2 days

#### Structure

```ruby
class User
  include FraiseQL::Type

  field :id, :ID
  field :name, :String
  field :email, :String, null: true
end

module Queries
  include FraiseQL::QueryBuilder

  query :users, User, array: true, sql_source: "v_user"
  query :user, User, sql_source: "v_user"
end

FraiseQL.export("types.json", [User, Queries])
```

#### Effort Estimate: 1-2 days

---

### Kotlin Implementation

**Current State**: 1,256 LOC, 9 tests (only security)
**Target State**: 600 LOC
**Timeline**: 1-2 days

#### Structure

```kotlin
@GraphQLType
data class User(
    @GraphQLField val id: String,
    @GraphQLField val name: String,
    @GraphQLField val email: String? = null
)

object Queries {
    @GraphQLQuery(sqlSource = "v_user")
    fun users(limit: Int = 10): List<User> = emptyList()
}

fun main() {
    FraiseQL.export("types.json", User::class, Queries::class)
}
```

#### Effort Estimate: 1-2 days

---

### C#/.NET Implementation

**Current State**: 1,384 LOC, 7 tests (only security)
**Target State**: 600 LOC
**Timeline**: 1-2 days

#### Structure

```csharp
[GraphQLType]
public class User
{
    [GraphQLField]
    public string Id { get; set; }

    [GraphQLField]
    public string Name { get; set; }

    [GraphQLField(Nullable = true)]
    public string? Email { get; set; }
}

public static class Queries
{
    [GraphQLQuery(SqlSource = "v_user")]
    public static List<User> Users(int limit = 10) => new();
}

// Export
FraiseQL.ExportSchema("types.json", typeof(User), typeof(Queries));
```

#### Effort Estimate: 1-2 days

---

### Rust Implementation

**Current State**: 1,547 LOC, 2 tests (only security)
**Target State**: 500 LOC
**Timeline**: 1-2 days

#### Structure

```rust
use fraiseql::*;

#[derive(GraphQLType)]
pub struct User {
    id: String,
    name: String,
    #[graphql(nullable)]
    email: Option<String>,
}

pub struct Queries;

#[fraiseql_queries]
impl Queries {
    #[query(sql_source = "v_user")]
    async fn users(&self, limit: i32) -> Vec<User> {
        vec![]
    }
}

fn main() -> Result<()> {
    FraiseQL::export("types.json", &[
        Type::from::<User>(),
        Type::from::<Queries>(),
    ])?;
    Ok(())
}
```

#### Effort Estimate: 1-2 days

---

## Tier 3: Minimal Support (Node.js, Dart, Elixir, Swift)

These use simple builder APIs with no decorators.

### Node.js Implementation

**Current State**: 1,436 LOC, 5 tests (only security)
**Target State**: 400 LOC
**Timeline**: 1 day

#### Structure

```typescript
import * as fraiseql from "fraiseql";

const User = fraiseql.type("User")
  .field("id", "ID")
  .field("name", "String")
  .field("email", "String", { nullable: true });

const queries = fraiseql.queries()
  .add("users", User, { sqlSource: "v_user", array: true })
  .add("user", User, { sqlSource: "v_user" });

// Export
fraiseql.export("types.json", [User, queries]);
```

#### Effort Estimate: 1 day

---

### Dart Implementation

**Current State**: 1,111 LOC, 3 tests
**Target State**: 400 LOC
**Timeline**: 1 day

#### Structure

```dart
import 'package:fraiseql/fraiseql.dart';

final user = GraphQLType('User')
  .field('id', 'ID')
  .field('name', 'String')
  .field('email', 'String', nullable: true);

final queries = GraphQLQueries()
  .add('users', user, sqlSource: 'v_user', array: true)
  .add('user', user, sqlSource: 'v_user');

void main() {
  FraiseQL.export('types.json', [user, queries]);
}
```

#### Effort Estimate: 1 day

---

### Elixir Implementation

**Current State**: 296 LOC, 3 tests
**Target State**: 300 LOC
**Timeline**: 1 day

#### Structure

```elixir
defmodule MyApp.Schema do
  alias FraiseQL.Schema

  def user_type do
    Schema.type("User")
    |> Schema.field("id", "ID")
    |> Schema.field("name", "String")
    |> Schema.field("email", "String", nullable: true)
  end

  def queries do
    Schema.queries()
    |> Schema.add("users", user_type(), sql_source: "v_user", array: true)
    |> Schema.add("user", user_type(), sql_source: "v_user")
  end

  def export do
    Schema.export("types.json", [user_type(), queries()])
  end
end
```

#### Effort Estimate: 1 day

---

### Swift Implementation

**Current State**: 1,197 LOC, 0 tests
**Target State**: 400 LOC
**Timeline**: 1 day

#### Structure

```swift
import FraiseQL

let user = GraphQLType(name: "User")
  .field("id", type: "ID")
  .field("name", type: "String")
  .field("email", type: "String", nullable: true)

let queries = GraphQLQueries()
  .add("users", returns: user, sqlSource: "v_user", array: true)
  .add("user", returns: user, sqlSource: "v_user")

do {
  try FraiseQL.export("types.json", [user, queries])
} catch {
  print("Export failed: \(error)")
}
```

#### Effort Estimate: 1 day

---

## Tier 4: Planned (Scala, Groovy, Clojure)

These will be **YAML-only** (no language-specific implementation).

Users define schemas in YAML:

```yaml
# schema.yaml
types:
  User:
    fields:
      id: ID
      name: String
      email: String?

queries:
  users:
    return_type: User
    return_array: true
    sql_source: v_user
```

Then include in TOML:

```toml
[schema]
definitions = "schema.yaml"
```

**No language support needed** - YAML + TOML is sufficient.

---

## CLI Enhancement

The `fraiseql-cli` must be updated to support TOML-based workflow.

### Current Workflow
```bash
python schema.py → schema.json
fraiseql-cli compile schema.json → schema.compiled.json
```

### New Workflow
```bash
# Option 1: Language + TOML
python schema.py → types.json
fraiseql-cli compile fraiseql.toml --types types.json → schema.compiled.json

# Option 2: TOML only
fraiseql-cli compile fraiseql.toml → schema.compiled.json

# Option 3: YAML + TOML (Tier 4)
fraiseql-cli compile fraiseql.toml → schema.compiled.json
```

### Implementation Steps

#### Step 1: TOML Parser
```rust
// In fraiseql-cli
use toml;

fn parse_config(toml_path: &str) -> Result<Config> {
    let contents = std::fs::read_to_string(toml_path)?;
    toml::from_str(&contents)
}
```

#### Step 2: Merge Types + TOML
```rust
fn merge_schema(
    types: Option<&str>,  // types.json path
    config: Config,       // fraiseql.toml parsed
) -> Result<Schema> {
    // If types.json provided:
    //   Load types from JSON
    //   Merge with TOML type definitions
    //   Resolve sql_source, descriptions, etc.
    // If only TOML:
    //   Use inline type definitions
    // Return complete Schema
}
```

#### Step 3: Validation & Compilation
```rust
fn compile(schema: Schema) -> Result<CompiledSchema> {
    // Same as before - validate + compile
    validate(&schema)?;
    compile_to_json(&schema)
}
```

**Effort Estimate**: 3-5 days for full implementation

---

## Summary: Implementation Roadmap

| Phase | Component | Timeline | Effort |
|-------|-----------|----------|--------|
| **1** | TOML Schema Design | 1 day | ✅ DONE |
| **2** | CLI TOML Support | 3-5 days | 🔴 To Do |
| **3** | Tier 1 Updates (Python/TS/Java) | 5 days | 🔴 To Do |
| **4** | Tier 2 Implementation (6 langs) | 10-12 days | 🔴 To Do |
| **5** | Tier 3 Implementation (4 langs) | 4-5 days | 🔴 To Do |
| **6** | Tier 4 YAML Support | 2 days | 🔴 To Do |
| **7** | Documentation + Examples | 2-3 days | 🔴 To Do |
| **8** | Testing + Verification | 3 days | 🔴 To Do |
| **TOTAL** | **All 16 languages + TOML** | **~40 days** | **~6 weeks** |

---

## Success Criteria

### For Each Language Tier

#### Tier 1 (Python, TypeScript, Java)
- ✅ Reduced to <4000 LOC each
- ✅ Export types.json (not full schema.json)
- ✅ All config moves to TOML
- ✅ Existing decorators still work
- ✅ Updated README with TOML examples
- ✅ Tests pass for type/query definitions

#### Tier 2 (Go, PHP, Ruby, Kotlin, C#, Rust)
- ✅ Implemented in 500-1000 LOC each
- ✅ Generate types.json from language structures
- ✅ Pass 10+ tests covering type system
- ✅ README with quick start
- ✅ Work with fraiseql-cli

#### Tier 3 (Node.js, Dart, Elixir, Swift)
- ✅ Implemented in 300-500 LOC each
- ✅ Builder API for type/query definition
- ✅ Generate types.json
- ✅ Pass 5+ tests
- ✅ README with examples

#### Tier 4 (Scala, Groovy, Clojure)
- ✅ YAML schema support
- ✅ TOML includes YAML definitions
- ✅ fraiseql-cli merges YAML + TOML
- ✅ No language-specific code needed

### Cross-Cutting
- ✅ fraiseql-cli supports TOML + types.json merger
- ✅ All 16 languages produce compatible types.json
- ✅ Complete TOML documentation
- ✅ Examples for every feature
- ✅ Migration guide from old to new approach
- ✅ CI/CD gates verify each language implementation

---

## Next Actions

1. **Commit this plan** - Get stakeholder approval
2. **Start CLI enhancement** - TOML parser + merger
3. **Update Tier 1 languages** - Remove config code
4. **Implement Tier 2** - Go, PHP, Ruby, Kotlin, C#, Rust
5. **Implement Tier 3** - Node.js, Dart, Elixir, Swift
6. **Add Tier 4 support** - YAML in fraiseql-cli
7. **Document everything** - Complete guides + examples
8. **Release v2.0.0** - With all 16 languages at Tier 2+

---

**This plan enables complete 16-language support in ~6 weeks with clear implementation path for each tier.**
