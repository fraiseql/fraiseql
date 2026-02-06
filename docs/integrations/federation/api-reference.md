<!-- Skip to main content -->
---

title: FraiseQL Federation API Reference
description: Complete API reference for Python, TypeScript, and Rust federation support.
keywords: ["framework", "directives", "types", "sdk", "schema", "scalars", "monitoring", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL Federation API Reference

Complete API reference for Python, TypeScript, and Rust federation support.

## Table of Contents

1. [Python Federation API](#python-federation-api)
2. [TypeScript Federation API](#typescript-federation-api)
3. [Rust Federation API](#rust-federation-api)
4. [Configuration](#configuration)
5. [Error Handling](#error-handling)

---

## Python Federation API

### Decorators

#### @FraiseQL.type

Marks a class as a GraphQL type.

```python
<!-- Code example in Python -->
from FraiseQL import type

@type
class User:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
```text
<!-- Code example in TEXT -->

**Parameters**: None

**Returns**: Type-decorated class

---

#### @FraiseQL.key

Declares the primary key field(s) for federation.

```python
<!-- Code example in Python -->
from FraiseQL import type, key

@type
@key(fields=["id"])
class User:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
```text
<!-- Code example in TEXT -->

**Parameters**:

- `fields: list[str]` - Field names that comprise the key (supports composite keys)

**Multiple Keys** (federation v2):

```python
<!-- Code example in Python -->
@type
@key(fields=["id"])
@key(fields=["email"])
class User:
    id: UUID  # UUID v4 for GraphQL ID
    email: str
    name: str
```text
<!-- Code example in TEXT -->

**Composite Keys** (for multi-tenant):

```python
<!-- Code example in Python -->
@type
@key(fields=["organization_id", "user_id"])
class User:
    organization_id: UUID  # UUID v4 for GraphQL ID
    user_id: UUID  # UUID v4 for GraphQL ID
    name: str
```text
<!-- Code example in TEXT -->

---

#### @FraiseQL.extends

Marks that this service extends an entity from another service.

```python
<!-- Code example in Python -->
from FraiseQL import type, extends, external, key

@type
@extends
@key(fields=["id"])
class User:
    id: str = external()
    orders: list["Order"]
```text
<!-- Code example in TEXT -->

**Parameters**: None

**Notes**:

- Can only be used once per type
- Must be combined with `@key`
- Extended fields must be marked with `@external()`

---

#### @FraiseQL.external

Marks a field as external (owned by another subgraph).

```python
<!-- Code example in Python -->
from FraiseQL import type, extends, external

@type
@extends
class User:
    id: str = external()
    email: str = external()
    orders: list["Order"]
```text
<!-- Code example in TEXT -->

**Parameters**: None

**Usage**:

- Mark fields that come from the authoritative subgraph
- Can only be used on extended types
- External fields should match the authoritative schema

---

#### @FraiseQL.requires

Specifies fields needed from the authoritative subgraph to resolve this field.

```python
<!-- Code example in Python -->
from FraiseQL import type, extends, external, requires

@type
@extends
class User:
    id: str = external()
    email: str = external()
    orders: list["Order"] = requires(fields=["id"])
```text
<!-- Code example in TEXT -->

**Parameters**:

- `fields: list[str]` - Field names needed from authoritative subgraph

**Example: Conditional Resolution**:

```python
<!-- Code example in Python -->
@type
@extends
class Product:
    id: str = external()
    is_available: bool = requires(fields=["stock_level"])
```text
<!-- Code example in TEXT -->

---

#### @FraiseQL.provides

Specifies fields that this subgraph can provide to resolver.

```python
<!-- Code example in Python -->
from FraiseQL import type, provides

@type
class Order:
    id: UUID  # UUID v4 for GraphQL ID
    user_id: UUID  # UUID v4 for GraphQL ID
    total: float = provides(from_fields=["user_id"])
```text
<!-- Code example in TEXT -->

**Parameters**:

- `from_fields: list[str]` - Fields used to compute the provided field

---

#### @FraiseQL.shareable

Marks that multiple subgraphs can provide this field.

```python
<!-- Code example in Python -->
from FraiseQL import type, shareable

@type
class Product:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
    price: float = shareable()
```text
<!-- Code example in TEXT -->

**Parameters**: None

**Usage**:

- Allows multiple services to implement the same field
- Useful for overrides or specialized implementations
- Federation gateway decides which to use

---

### Field Types

#### Basic Types

```python
<!-- Code example in Python -->
from FraiseQL import type

@type
class User:
    id: str                    # String
    age: int                   # Integer
    height: float              # Float
    verified: bool             # Boolean
    metadata: dict             # Object/JSON
    tags: list[str]            # Array
```text
<!-- Code example in TEXT -->

#### Optional Fields

```python
<!-- Code example in Python -->
from FraiseQL import type
from typing import Optional

@type
class User:
    id: UUID  # UUID v4 for GraphQL ID
    email: Optional[str]       # Nullable field (Python 3.10+)
    phone: str | None          # Also valid (3.10+ preferred)
```text
<!-- Code example in TEXT -->

#### ID Type

```python
<!-- Code example in Python -->
from FraiseQL import type, ID

@type
@key(fields=["id"])
class User:
    id: ID                     # Special ID scalar
    name: str
```text
<!-- Code example in TEXT -->

#### Custom Scalars

```python
<!-- Code example in Python -->
from FraiseQL import type, scalar

DateTime = scalar("DateTime", description="ISO 8601 datetime")

@type
class User:
    id: UUID  # UUID v4 for GraphQL ID
    created_at: DateTime
```text
<!-- Code example in TEXT -->

---

### Query Definition

```python
<!-- Code example in Python -->
from FraiseQL import type
from typing import Optional

@type
class Query:
    """Root query type"""

    def user(self, id: str) -> Optional["User"]:
        """Get user by ID"""
        pass

    def users(self) -> list["User"]:
        """Get all users"""
        pass

    def users_by_name(self, name: str) -> list["User"]:
        """Get users by name"""
        pass
```text
<!-- Code example in TEXT -->

**Parameter Types**:

- `str`, `int`, `float`, `bool` - Scalar arguments
- `ID` - Special ID type
- `Optional[T]` - Nullable argument
- `list[T]` - List argument

**Return Types**:

- `T` - Single entity (non-nullable)
- `Optional[T]` - Nullable entity
- `list[T]` - List of entities
- `list[Optional[T]]` - List with nullable elements

---

### Mutation Definition

```python
<!-- Code example in Python -->
from FraiseQL import type
from typing import Optional

@type
class Mutation:
    """Root mutation type"""

    def create_user(self, name: str, email: str) -> "User":
        """Create new user"""
        pass

    def update_user(self, id: str, name: Optional[str] = None) -> Optional["User"]:
        """Update user"""
        pass

    def delete_user(self, id: str) -> bool:
        """Delete user"""
        pass
```text
<!-- Code example in TEXT -->

---

### Complete Python Example

```python
<!-- Code example in Python -->
"""Users Service with Federation"""
from FraiseQL import type, key, extends, external, requires, ID
from typing import Optional

@type
@key(fields=["id"])
class User:
    id: ID
    email: str
    name: str

@type
@extends
@key(fields=["id"])
class Order:
    id: ID = external()
    user_id: ID = external()
    user: User = requires(fields=["id"])

@type
class Query:
    def user(self, id: ID) -> Optional[User]:
        pass

    def users(self) -> list[User]:
        pass

@type
class Mutation:
    def create_user(self, email: str, name: str) -> User:
        pass

    def update_user(
        self,
        id: ID,
        email: Optional[str] = None,
        name: Optional[str] = None
    ) -> Optional[User]:
        pass
```text
<!-- Code example in TEXT -->

---

## TypeScript Federation API

### Decorators

#### @Type

Marks a class as a GraphQL type.

```typescript
<!-- Code example in TypeScript -->
import { Type, Key } from '@FraiseQL/typescript';

@Type()
class User {
  id: string;
  name: string;
}
```text
<!-- Code example in TEXT -->

---

#### @Key

Declares federation key field(s).

```typescript
<!-- Code example in TypeScript -->
import { Type, Key } from '@FraiseQL/typescript';

@Type()
@Key({ fields: ['id'] })
class User {
  id: string;
  name: string;
}
```text
<!-- Code example in TEXT -->

**Parameters**:

- `fields: string[]` - Key field names
- Composite keys supported: `@Key({ fields: ['org_id', 'user_id'] })`
- Multiple keys supported: `@Key(...) @Key(...)`

---

#### @Extends

Marks that this service extends an entity.

```typescript
<!-- Code example in TypeScript -->
import { Type, Extends, Key, External } from '@FraiseQL/typescript';

@Type()
@Extends()
@Key({ fields: ['id'] })
class User {
  @External()
  id: string;

  orders: Order[];
}
```text
<!-- Code example in TEXT -->

---

#### @External

Marks a field as external (owned by another service).

```typescript
<!-- Code example in TypeScript -->
@Type()
@Extends()
class User {
  @External()
  id: string;

  @External()
  email: string;
}
```text
<!-- Code example in TEXT -->

---

#### @Requires

Specifies fields needed to resolve this field.

```typescript
<!-- Code example in TypeScript -->
import { Requires } from '@FraiseQL/typescript';

@Type()
@Extends()
class User {
  @External()
  id: string;

  @Requires({ fields: ['id'] })
  orders: Order[];
}
```text
<!-- Code example in TEXT -->

---

#### @Shareable

Marks that multiple services provide this field.

```typescript
<!-- Code example in TypeScript -->
import { Shareable } from '@FraiseQL/typescript';

@Type()
class Product {
  id: string;
  name: string;

  @Shareable()
  price: number;
}
```text
<!-- Code example in TEXT -->

---

### Field Types

```typescript
<!-- Code example in TypeScript -->
@Type()
class User {
  id: string;                   // String
  age: number;                  // Number
  verified: boolean;            // Boolean
  metadata: Record<string, any>;// Object
  tags: string[];              // Array
  createdAt?: string;          // Optional (nullable)
  email: string | null;        // Union with null
}
```text
<!-- Code example in TEXT -->

---

### Query Definition

```typescript
<!-- Code example in TypeScript -->
import { Type } from '@FraiseQL/typescript';

@Type()
class Query {
  user(id: string): User | null {
    // Implementation
    return null;
  }

  users(): User[] {
    // Implementation
    return [];
  }
}
```text
<!-- Code example in TEXT -->

---

### Mutation Definition

```typescript
<!-- Code example in TypeScript -->
import { Type } from '@FraiseQL/typescript';

@Type()
class Mutation {
  createUser(email: string, name: string): User {
    // Implementation
    return {} as User;
  }

  updateUser(
    id: string,
    email?: string,
    name?: string
  ): User | null {
    // Implementation
    return null;
  }

  deleteUser(id: string): boolean {
    // Implementation
    return true;
  }
}
```text
<!-- Code example in TEXT -->

---

### Complete TypeScript Example

```typescript
<!-- Code example in TypeScript -->
import {
  Type,
  Key,
  Extends,
  External,
  Requires,
  Shareable,
} from '@FraiseQL/typescript';

@Type()
@Key({ fields: ['id'] })
class User {
  id: string;
  email: string;
  name: string;
}

@Type()
@Extends()
@Key({ fields: ['id'] })
class Order {
  @External()
  id: string;

  @External()
  user_id: string;

  @Requires({ fields: ['id'] })
  user: User;
}

@Type()
class Query {
  user(id: string): User | null {
    return null;
  }

  users(): User[] {
    return [];
  }
}

@Type()
class Mutation {
  createUser(email: string, name: string): User {
    return {} as User;
  }

  updateUser(
    id: string,
    email?: string,
    name?: string
  ): User | null {
    return null;
  }
}
```text
<!-- Code example in TEXT -->

---

## Rust Federation API

### Core Types

#### FederationMetadata

```rust
<!-- Code example in RUST -->
pub struct FederationMetadata {
    pub enabled: bool,
    pub version: String,
    pub types: Vec<FederatedType>,
}

pub struct FederatedType {
    pub name: String,
    pub keys: Vec<KeyDirective>,
    pub is_extends: bool,
    pub external_fields: Vec<String>,
    pub shareable_fields: Vec<String>,
}
```text
<!-- Code example in TEXT -->

---

#### EntityRepresentation

```rust
<!-- Code example in RUST -->
pub struct EntityRepresentation {
    pub typename: String,
    pub key_fields: HashMap<String, Value>,
}
```text
<!-- Code example in TEXT -->

Used to represent entities in `_entities` queries.

---

#### ResolutionStrategy

```rust
<!-- Code example in RUST -->
pub enum ResolutionStrategy {
    Local {
        view_name: String,
        key_columns: Vec<String>,
    },
    Http {
        subgraph_url: String,
    },
    DirectDatabase {
        connection_string: String,
        key_columns: Vec<String>,
    },
}
```text
<!-- Code example in TEXT -->

---

### FederationResolver

```rust
<!-- Code example in RUST -->
pub struct FederationResolver {
    pub metadata: FederationMetadata,
    pub config: FederationConfig,
}

impl FederationResolver {
    /// Create new federation resolver
    pub fn new(
        metadata: FederationMetadata,
        config: FederationConfig,
    ) -> Result<Self>;

    /// Get resolution strategy for type
    pub fn get_or_determine_strategy(
        &self,
        typename: &str,
    ) -> Result<ResolutionStrategy>;

    /// Resolve entities batch
    pub async fn resolve_entities(
        &self,
        representations: &[EntityRepresentation],
        typename: &str,
        selection: &FieldSelection,
    ) -> Result<Vec<Option<Value>>>;
}
```text
<!-- Code example in TEXT -->

---

### EntityResolver

```rust
<!-- Code example in RUST -->
pub async fn resolve_entities_by_strategy(
    representations: &[EntityRepresentation],
    typename: &str,
    fed_resolver: &FederationResolver,
    local_adapter: Arc<dyn DatabaseAdapter>,
    selection: &FieldSelection,
) -> EntityResolutionResult;

pub struct EntityResolutionResult {
    pub entities: Vec<Option<Value>>,
    pub errors: Vec<String>,
}
```text
<!-- Code example in TEXT -->

---

### HTTP Resolution

```rust
<!-- Code example in RUST -->
pub struct HttpEntityResolver {
    client: Option<reqwest::Client>,
    config: HttpClientConfig,
}

impl HttpEntityResolver {
    pub async fn resolve_entities(
        &self,
        subgraph_url: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<Option<Value>>>;
}
```text
<!-- Code example in TEXT -->

---

### Mutation HTTP Client

```rust
<!-- Code example in RUST -->
pub struct HttpMutationClient {
    client: Option<reqwest::Client>,
    config: HttpMutationConfig,
}

impl HttpMutationClient {
    pub async fn execute_mutation(
        &self,
        url: &str,
        typename: &str,
        mutation_name: &str,
        variables: &Value,
    ) -> Result<Value>;
}
```text
<!-- Code example in TEXT -->

---

## Configuration

### TOML Configuration

Create `federation.toml`:

```toml
<!-- Code example in TOML -->
[federation]
enabled = true

# Subgraph definitions
[[federation.subgraphs]]
name = "User"
strategy = "local"

[[federation.subgraphs]]
name = "Order"
strategy = "http"
url = "http://orders-service:4000/graphql"

[[federation.subgraphs]]
name = "Product"
strategy = "direct-database"
database_url = "postgresql://localhost/products"

# HTTP client configuration
[federation.http]
timeout_ms = 5000
max_retries = 3
retry_delay_ms = 100

# Connection pool configuration
[federation.db]
pool_size = 20
max_idle_time = 300
connection_timeout = 5
```text
<!-- Code example in TEXT -->

---

### Runtime Configuration

```rust
<!-- Code example in RUST -->
pub struct FederationConfig {
    pub enabled: bool,
    pub subgraphs: Vec<SubgraphConfig>,
    pub http: HttpClientConfig,
}

pub struct SubgraphConfig {
    pub name: String,
    pub strategy: SubgraphStrategy,
    pub url: Option<String>,
    pub database_url: Option<String>,
}

pub struct HttpClientConfig {
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
}
```text
<!-- Code example in TEXT -->

---

## Error Handling

### Python

```python
<!-- Code example in Python -->
from FraiseQL import FraiseQLError

try:
    user = resolver.resolve_entity("User", representation)
except FraiseQLError as e:
    print(f"Federation error: {e}")
```text
<!-- Code example in TEXT -->

---

### TypeScript

```typescript
<!-- Code example in TypeScript -->
try {
  const user = await resolver.resolveEntity('User', representation);
} catch (error) {
  if (error instanceof FraiseQLError) {
    console.error('Federation error:', error.message);
  }
}
```text
<!-- Code example in TEXT -->

---

### Rust

```rust
<!-- Code example in RUST -->
match resolver.resolve_entities(reps, "User", selection).await {
    Ok(entities) => {
        // Handle entities
    }
    Err(e) => {
        eprintln!("Federation error: {}", e);
    }
}
```text
<!-- Code example in TEXT -->

---

## Best Practices

### Python

1. **Type Annotations**: Always use type hints

```python
<!-- Code example in Python -->
@type
@key(fields=["id"])
class User:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
    email: str | None
```text
<!-- Code example in TEXT -->

1. **Composite Keys**: Clearly document when using

```python
<!-- Code example in Python -->
@type
@key(fields=["organization_id", "user_id"])
class User:
    organization_id: UUID  # UUID v4 for GraphQL ID
    user_id: UUID  # UUID v4 for GraphQL ID
```text
<!-- Code example in TEXT -->

1. **External Fields**: Mark all external fields

```python
<!-- Code example in Python -->
@type
@extends
class Order:
    id: str = external()
    user_id: str = external()
```text
<!-- Code example in TEXT -->

### TypeScript

1. **Decorator Order**: Key decorators before field decorators

```typescript
<!-- Code example in TypeScript -->
@Type()
@Key({ fields: ['id'] })
@Extends()
class User {
  @External()
  id: string;
}
```text
<!-- Code example in TEXT -->

1. **Null Safety**: Use strict null checks

```typescript
<!-- Code example in TypeScript -->
@Type()
class User {
  id: string;         // Non-nullable
  email: string | null; // Nullable
}
```text
<!-- Code example in TEXT -->

1. **Interfaces**: Create interfaces for complex types

```typescript
<!-- Code example in TypeScript -->
interface UserInput {
  email: string;
  name: string;
}

@Type()
class Mutation {
  createUser(input: UserInput): User {
    return {} as User;
  }
}
```text
<!-- Code example in TEXT -->

### Rust

1. **Error Handling**: Use `Result<T>` consistently

```rust
<!-- Code example in RUST -->
pub async fn resolve_entities(
    &self,
    representations: &[EntityRepresentation],
) -> Result<Vec<Option<Value>>>;
```text
<!-- Code example in TEXT -->

1. **Async/Await**: Mark async functions properly

```rust
<!-- Code example in RUST -->
pub async fn resolve_entities_by_strategy(...) -> Result<...>;
```text
<!-- Code example in TEXT -->

1. **Arc<dyn Trait>**: Use for database adapters

```rust
<!-- Code example in RUST -->
pub fn new(adapter: Arc<dyn DatabaseAdapter>) -> Self;
```text
<!-- Code example in TEXT -->

---

## Troubleshooting

### Python: "Type not registered in federation"

**Solution**: Ensure `@type` decorator is applied before `@key`

### TypeScript: "Decorator order invalid"

**Solution**: Apply class decorators before field decorators

### Rust: "Strategy not found for type"

**Solution**: Ensure configuration includes all federation types

---

## See Also

- [Federation Guide](guide.md)
- [Deployment Guide](deployment.md)
- [Examples](../../../examples/federation/)
