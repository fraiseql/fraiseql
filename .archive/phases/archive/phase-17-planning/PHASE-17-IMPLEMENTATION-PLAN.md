# Phase 17: Apollo Federation Implementation Plan
## Architecture & Technical Design

**Date**: January 2, 2026
**Phase**: 17 (Apollo Federation Support)
**Duration**: 5-6 weeks
**Effort**: 150-180 hours

---

## 🎯 Overview

Implement **Apollo Federation 2.0 support** for FraiseQL with three progressive levels:
- **Federation Lite** (80% of users): Auto-key detection, `@entity` decorator
- **Federation Standard** (15% of users): Type extensions, `@requires`, `@provides`
- **Federation Advanced** (5% of users): All 18 directives (Phase 17b)

---

## 🏗️ Architecture

### Integration Points

```
FraiseQL Federation Architecture
================================

Python Layer (src/fraiseql/federation/)
  ├─ @entity decorator (auto-key detection)
  ├─ @extend_entity decorator (type extensions)
  ├─ FederationConfig class
  └─ Presets (LITE, STANDARD, ADVANCED)
        ↓
        ↓ (Python ↔ Rust bridge via PyO3)
        ↓
Rust Layer (fraiseql_rs/src/federation/)
  ├─ auto_detect.rs (key field detection)
  ├─ lite.rs (lightweight resolver generation)
  ├─ directives.rs (directive parsing)
  ├─ entities_resolver.rs (auto-generated _entities)
  ├─ sdl_generator.rs (schema generation)
  ├─ batch_loader.rs (DataLoader pattern)
  └─ py_bindings.rs (Python interface)
        ↓
        ↓ (via existing pipeline)
        ↓
Existing FraiseQL Core
  ├─ GraphQL schema builder
  ├─ PostgreSQL pipeline
  └─ Response builder
```

### Key Design Decisions

1. **Auto-Detection First**: Detect `id` field automatically, fail gracefully if missing
2. **Rust Performance**: Federation operations in Rust for < 2ms entity resolution
3. **Zero-Config**: `federation=True` enables everything, auto-detects all `@entity` classes
4. **Progressive Disclosure**: Lite → Standard → Advanced modes with clear upgrade path
5. **Batch Loading**: Auto-batching via DataLoader pattern (N+1 problem solved)

---

## 📋 Week-by-Week Implementation Plan

### Week 1: Federation Lite (30-35 hours)

#### Day 1-2: Auto-Key Detection (6-8 hours)

**Objective**: Implement Rust-based key field detection

**Files to Create**:
- `fraiseql_rs/src/federation/mod.rs` - Module exports
- `fraiseql_rs/src/federation/auto_detect.rs` - Key detection logic

**Implementation**:

```rust
// fraiseql_rs/src/federation/auto_detect.rs
use std::collections::HashMap;

/// Auto-detect entity key field from type definition
pub fn auto_detect_key(
    type_name: &str,
    fields: &HashMap<String, FieldInfo>,
) -> Result<String, AutoDetectError> {
    // Priority order:
    // 1. Field named 'id' (most common, 90% of cases)
    // 2. Field with @primary_key annotation
    // 3. First field with ID scalar type
    // 4. None - error with clear message

    if fields.contains_key("id") {
        return Ok("id".to_string());
    }

    // Check for primary_key annotation
    for (field_name, field_info) in fields {
        if field_info.annotations.contains("primary_key") {
            return Ok(field_name.clone());
        }
    }

    // Check for ID scalar type
    for (field_name, field_info) in fields {
        if field_info.type_name == "ID" || field_info.type_name == "ID!" {
            return Ok(field_name.clone());
        }
    }

    Err(AutoDetectError::NoKeyFound {
        type_name: type_name.to_string(),
    })
}

#[derive(Debug)]
pub enum AutoDetectError {
    NoKeyFound { type_name: String },
}

pub struct FieldInfo {
    pub type_name: String,
    pub annotations: Vec<String>,
    pub is_required: bool,
}
```

**Python Integration**:
```python
# src/fraiseql/federation/auto_detect.py
from typing import Optional

def auto_detect_key_python(cls: type) -> Optional[str]:
    """Auto-detect key field from Python class annotations."""
    annotations = getattr(cls, '__annotations__', {})

    # Check for 'id' field (most common)
    if 'id' in annotations:
        return 'id'

    # Check for common patterns
    for field in ['uuid', 'pk', 'primary_key', '_id']:
        if field in annotations:
            return field

    return None
```

**Testing**:
```python
# tests/federation/test_auto_detect.py
def test_auto_detect_id_field():
    @entity
    class User:
        id: str
        name: str

    # Should auto-detect 'id' as key
    assert get_entity_key(User) == 'id'

def test_auto_detect_no_id_field():
    @entity(key="user_id")
    class User:
        user_id: str
        name: str

    # Should use explicit key
    assert get_entity_key(User) == 'user_id'

def test_auto_detect_error():
    with pytest.raises(ValueError, match="No 'id' field"):
        @entity
        class User:
            name: str
```

**Acceptance Criteria**:
- [ ] Auto-detects `id` field as key
- [ ] Works with 90% of models (simple case)
- [ ] Clear error message when no key found
- [ ] All tests pass

---

#### Day 3-4: Simple Python API (8-10 hours)

**Objective**: Implement `@entity` decorator with auto-key detection

**Files to Create**:
- `src/fraiseql/federation/__init__.py` - Module initialization
- `src/fraiseql/federation/decorators.py` - Entity decorators
- `src/fraiseql/federation/config.py` - Configuration classes

**Implementation**:

```python
# src/fraiseql/federation/decorators.py
from typing import Optional, Union, List, Any
from typing_extensions import overload

class EntityMetadata:
    """Metadata for a federated entity."""
    def __init__(
        self,
        cls: type,
        key: Optional[Union[str, List[str]]] = None,
    ):
        self.cls = cls
        self.type_name = cls.__name__
        self.key = key
        self.resolved_key = self._resolve_key()
        self.fields = self._extract_fields()

    def _resolve_key(self) -> Union[str, List[str]]:
        """Resolve key: explicit > auto-detected > error."""
        if self.key is not None:
            return self.key

        # Auto-detect
        from .auto_detect import auto_detect_key_python
        detected = auto_detect_key_python(self.cls)

        if detected is None:
            raise ValueError(
                f"{self.type_name} has no 'id' field. "
                f"Specify key explicitly: @entity(key='field_name')"
            )

        return detected

    def _extract_fields(self) -> dict[str, type]:
        """Extract field annotations from class."""
        annotations = getattr(self.cls, '__annotations__', {})
        return {
            name: annotation
            for name, annotation in annotations.items()
        }

# Global registry of entities
_ENTITY_REGISTRY: dict[str, EntityMetadata] = {}

@overload
def entity(cls: type) -> type: ...

@overload
def entity(
    *,
    key: Optional[Union[str, List[str]]] = None,
) -> callable: ...

def entity(
    cls: Optional[type] = None,
    *,
    key: Optional[Union[str, List[str]]] = None,
):
    """Mark a type as a federated entity.

    Args:
        key: Entity key field(s). Auto-detected from 'id' if not provided.

    Examples:
        # Simple: Auto-detect key from 'id' field
        >>> @entity
        ... class User:
        ...     id: str
        ...     name: str

        # Explicit: Specify key
        >>> @entity(key="user_id")
        ... class User:
        ...     user_id: str

        # Composite: Multiple key fields
        >>> @entity(key=["org_id", "user_id"])
        ... class OrgUser:
        ...     org_id: str
        ...     user_id: str
    """
    def decorator(cls_to_decorate: type) -> type:
        # Create metadata
        metadata = EntityMetadata(cls_to_decorate, key=key)

        # Register entity
        _ENTITY_REGISTRY[metadata.type_name] = metadata

        # Store metadata on class for introspection
        cls_to_decorate.__fraiseql_entity__ = metadata

        return cls_to_decorate

    if cls is None:
        # Called with arguments: @entity(key="...")
        return decorator
    else:
        # Called without arguments: @entity
        return decorator(cls)

def extend_entity(
    cls: Optional[type] = None,
    *,
    key: Union[str, List[str]],
):
    """Mark a type as an extended federated entity.

    Used for entities defined in other subgraphs.

    Args:
        key: Reference key to parent entity.

    Example:
        >>> @extend_entity(key="id")
        ... class Product:
        ...     id: str = external()
        ...     reviews: list["Review"]
    """
    def decorator(cls_to_decorate: type) -> type:
        metadata = EntityMetadata(cls_to_decorate, key=key)
        metadata.is_extension = True

        _ENTITY_REGISTRY[metadata.type_name] = metadata
        cls_to_decorate.__fraiseql_entity__ = metadata

        return cls_to_decorate

    if cls is None:
        return decorator
    else:
        return decorator(cls)

def get_entity_registry() -> dict[str, EntityMetadata]:
    """Get all registered entities."""
    return _ENTITY_REGISTRY.copy()

def get_entity_metadata(type_name: str) -> Optional[EntityMetadata]:
    """Get metadata for a specific entity."""
    return _ENTITY_REGISTRY.get(type_name)
```

**Configuration**:

```python
# src/fraiseql/federation/config.py
from typing import Optional, List
from dataclasses import dataclass

@dataclass
class FederationConfig:
    """Configuration for Apollo Federation support."""

    # Basic settings
    enabled: bool = True
    version: str = "2.5"  # Apollo Federation version

    # Feature flags
    auto_keys: bool = True  # Auto-detect entity keys
    auto_entities_resolver: bool = True  # Auto-generate _entities
    auto_service_resolver: bool = True  # Auto-generate _service

    # Directives to support
    directives: List[str] = None  # List of supported directives

    # Performance
    batch_size: int = 100  # DataLoader batch size
    batch_window_ms: int = 10  # Wait time for batching (ms)

    # Caching
    cache_sdl: bool = True  # Cache generated SDL
    cache_ttl_seconds: Optional[int] = 3600  # SDL cache TTL

    def __post_init__(self):
        if self.directives is None:
            # Default to lite directives
            self.directives = ["key", "external"]

class Presets:
    """Federation configuration presets."""

    # Lite: Auto-keys only (80% of users)
    LITE = FederationConfig(
        version="2.5",
        auto_keys=True,
        directives=["key", "external"],
        batch_size=100,
        batch_window_ms=10,
    )

    # Standard: With extensions (15% of users)
    STANDARD = FederationConfig(
        version="2.5",
        auto_keys=True,
        directives=["key", "external", "requires", "provides"],
        batch_size=100,
        batch_window_ms=10,
    )

    # Advanced: All directives (5% of users, Phase 17b)
    ADVANCED = FederationConfig(
        version="2.5",
        auto_keys=False,
        directives=[
            "key", "external", "requires", "provides", "shareable",
            "override", "inaccessible", "tag", "interfaceObject",
        ],
        batch_size=100,
        batch_window_ms=10,
    )
```

**Testing**:
```python
# tests/federation/test_decorators.py
import pytest
from fraiseql.federation import entity, extend_entity, get_entity_registry

def test_entity_auto_key():
    @entity
    class User:
        id: str
        name: str

    registry = get_entity_registry()
    assert "User" in registry
    assert registry["User"].resolved_key == "id"

def test_entity_explicit_key():
    @entity(key="user_id")
    class User:
        user_id: str
        name: str

    registry = get_entity_registry()
    assert registry["User"].resolved_key == "user_id"

def test_entity_composite_key():
    @entity(key=["org_id", "user_id"])
    class OrgUser:
        org_id: str
        user_id: str

    registry = get_entity_registry()
    assert registry["OrgUser"].resolved_key == ["org_id", "user_id"]

def test_entity_no_key_error():
    with pytest.raises(ValueError, match="No 'id' field"):
        @entity
        class BadEntity:
            name: str
```

**Acceptance Criteria**:
- [ ] `@entity` works without arguments
- [ ] Auto-detects `id` field
- [ ] Explicit key parameter works
- [ ] Composite keys supported
- [ ] Clear error if no key found
- [ ] Type hints complete
- [ ] All tests pass

---

#### Day 5: Auto-Generated `_entities` Resolver (8-10 hours)

**Objective**: Auto-generate entity resolution from entity metadata

**Files to Create**:
- `fraiseql_rs/src/federation/entities_resolver.rs` - Entity resolver generation

**Implementation**:

```rust
// fraiseql_rs/src/federation/entities_resolver.rs
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct EntityMetadata {
    pub type_name: String,
    pub key_field: String,
    pub table_name: String,
    pub fields: HashMap<String, String>,  // field_name -> type_name
}

pub struct EntityResolver {
    entities: HashMap<String, EntityMetadata>,
}

impl EntityResolver {
    pub fn new(entities: Vec<EntityMetadata>) -> Self {
        let mut map = HashMap::new();
        for entity in entities {
            map.insert(entity.type_name.clone(), entity);
        }

        Self {
            entities: map,
        }
    }

    /// Auto-generate SQL query for entity resolution
    pub fn generate_query(
        &self,
        type_name: &str,
        key_value: &Value,
    ) -> Result<String, ResolutionError> {
        let entity = self.entities.get(type_name)
            .ok_or(ResolutionError::UnknownType(type_name.to_string()))?;

        // Generate parameterized query
        let query = format!(
            "SELECT * FROM {} WHERE {} = $1",
            entity.table_name,
            entity.key_field
        );

        Ok(query)
    }

    /// Auto-generate batch query for entity resolution
    pub fn generate_batch_query(
        &self,
        type_name: &str,
        key_count: usize,
    ) -> Result<String, ResolutionError> {
        let entity = self.entities.get(type_name)
            .ok_or(ResolutionError::UnknownType(type_name.to_string()))?;

        // Generate batch query: SELECT * FROM table WHERE key = ANY($1)
        let placeholders = (1..=key_count)
            .map(|i| format!("${}", i))
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!(
            "SELECT * FROM {} WHERE {} IN ({})",
            entity.table_name,
            entity.key_field,
            placeholders
        );

        Ok(query)
    }
}

#[derive(Debug)]
pub enum ResolutionError {
    UnknownType(String),
    InvalidKey(String),
    DatabaseError(String),
}
```

**Python Bridge**:

```python
# src/fraiseql/federation/entities.py
from typing import List, Dict, Any
from fraiseql_rs import EntityResolver as RustEntityResolver

class EntitiesResolver:
    """Auto-generated _entities resolver."""

    def __init__(self, entities_metadata: Dict[str, Any]):
        self.metadata = entities_metadata
        self.rust_resolver = RustEntityResolver.new(entities_metadata)

    async def resolve(
        self,
        representations: List[Dict[str, Any]],
        db_pool,
    ) -> List[Dict[str, Any]]:
        """Resolve entities from representations.

        Args:
            representations: List of entity references with __typename and key
            db_pool: Database connection pool

        Returns:
            List of resolved entities
        """
        # Group by type for batch loading
        by_type: Dict[str, List[Any]] = {}

        for rep in representations:
            type_name = rep.get('__typename')
            key_value = rep.get(self.metadata[type_name]['key_field'])

            if type_name not in by_type:
                by_type[type_name] = []

            by_type[type_name].append(key_value)

        # Batch load each type
        results = {}
        for type_name, keys in by_type.items():
            query = self.rust_resolver.generate_batch_query(type_name, len(keys))

            async with db_pool.acquire() as conn:
                rows = await conn.fetch(query, *keys)

            results[type_name] = [dict(row) for row in rows]

        # Return in original order
        resolved = []
        for rep in representations:
            type_name = rep.get('__typename')
            key_value = rep.get(self.metadata[type_name]['key_field'])

            # Find matching entity
            for entity in results[type_name]:
                if entity[self.metadata[type_name]['key_field']] == key_value:
                    resolved.append(entity)
                    break

        return resolved
```

**Testing**:
```python
# tests/federation/test_entities.py
@pytest.mark.asyncio
async def test_entities_resolver():
    @entity
    class User:
        id: str
        name: str

    # Create resolver
    resolver = EntitiesResolver.from_registry()

    # Create test data
    representations = [
        {'__typename': 'User', 'id': '123'},
        {'__typename': 'User', 'id': '456'},
    ]

    # Resolve
    result = await resolver.resolve(representations, db_pool)

    assert len(result) == 2
    assert result[0]['id'] == '123'
    assert result[1]['id'] == '456'

@pytest.mark.asyncio
async def test_entities_batch_loading():
    """Verify N+1 problem solved via batching."""
    @entity
    class User:
        id: str
        name: str

    resolver = EntitiesResolver.from_registry()

    # 100 entity requests
    representations = [
        {'__typename': 'User', 'id': str(i)}
        for i in range(100)
    ]

    # Should use single batch query, not 100
    result = await resolver.resolve(representations, db_pool)

    assert len(result) == 100
    # Verify only 1 query executed (via query logging)
```

**Acceptance Criteria**:
- [ ] `_entities` query generates correct SQL
- [ ] Batch loading works (single query for N entities)
- [ ] N+1 problem solved
- [ ] Results in correct order
- [ ] All tests pass

---

### Week 2: Federation Standard (35-40 hours)

#### Day 1-2: Directive Parsing (8-10 hours)

**Files to Create**:
- `fraiseql_rs/src/federation/directives.rs` - Core directive parsing
- `fraiseql_rs/src/federation/standard.rs` - Standard mode support

**Implementation Plan**:

Parse 4 core directives (extending to all 18 in Phase 17b):
1. `@key(fields: "...")` - Entity key
2. `@external` - External field reference
3. `@requires(fields: "...")` - Field dependencies
4. `@provides(fields: "...")` - Eager field loading

```rust
// fraiseql_rs/src/federation/directives.rs
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum FederationDirective {
    Key {
        fields: Vec<String>,
    },
    External,
    Requires {
        fields: Vec<String>,
    },
    Provides {
        fields: Vec<String>,
    },
}

pub struct DirectiveParser;

impl DirectiveParser {
    pub fn parse(directive_name: &str, args: &HashMap<String, String>) -> Option<FederationDirective> {
        match directive_name {
            "key" => {
                let fields = Self::parse_fields(&args.get("fields")?);
                Some(FederationDirective::Key { fields })
            },
            "external" => Some(FederationDirective::External),
            "requires" => {
                let fields = Self::parse_fields(&args.get("fields")?);
                Some(FederationDirective::Requires { fields })
            },
            "provides" => {
                let fields = Self::parse_fields(&args.get("fields")?);
                Some(FederationDirective::Provides { fields })
            },
            _ => None,
        }
    }

    fn parse_fields(fields_str: &str) -> Vec<String> {
        fields_str
            .split_whitespace()
            .map(|s| s.trim_matches(|c| c == '"' || c == '{' || c == '}'))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }
}
```

---

#### Day 3-4: Type Extensions & `@external` (8-10 hours)

**Python API**:

```python
# src/fraiseql/federation/decorators.py (extend)
from typing import Any

def external():
    """Mark field as external (defined in another subgraph)."""
    # Return a marker object that decorators can detect
    return _External()

class _External:
    """Marker for external fields."""
    def __repr__(self):
        return "<external>"

@extend_entity(key="id")
class Product:
    id: str = external()  # From other subgraph
    name: str = external()
    reviews: list["Review"]  # New field in this subgraph
```

**Testing**:
```python
# tests/federation/test_extensions.py
def test_extend_entity_with_external():
    @extend_entity(key="id")
    class Product:
        id: str = external()
        reviews: list["Review"]

    metadata = get_entity_metadata("Product")
    assert metadata.is_extension is True
    assert "id" in metadata.external_fields
    assert "reviews" not in metadata.external_fields

def test_type_extension_sdl():
    """Verify extend type directive generated."""
    @extend_entity(key="id")
    class Product:
        id: str = external()
        reviews: list["Review"]

    sdl = generate_sdl()
    assert "extend type Product" in sdl
    assert "@external" in sdl
```

---

#### Day 5: `@requires` & `@provides` (8-10 hours)

**Python API**:

```python
# src/fraiseql/federation/decorators.py (extend)
from typing import List
from functools import wraps

def requires(fields: List[str]):
    """Mark field as requiring other fields.

    Example:
        @entity
        class Product:
            price: float = external()
            weight: float = external()

            @requires(["price", "weight"])
            def shipping_cost(self) -> float:
                return self.price * 0.1 + self.weight * 0.05
    """
    def decorator(fn):
        fn.__fraiseql_requires__ = fields
        return fn
    return decorator

def provides(fields: List[str]):
    """Mark field as providing other fields."""
    def decorator(fn):
        fn.__fraiseql_provides__ = fields
        return fn
    return decorator
```

**Implementation**:
- Auto-fetch required fields from gateway
- Pass to resolver function
- Support computed fields that depend on external data

---

### Week 3: SDL & Gateway Integration (30-40 hours)

#### Day 1-2: Auto-SDL Generation (8-10 hours)

**Files to Create**:
- `fraiseql_rs/src/federation/sdl_generator.rs` - SDL generation

**Implementation**:

```rust
// fraiseql_rs/src/federation/sdl_generator.rs
use crate::graphql::schema::Schema;

pub struct SDLGenerator;

impl SDLGenerator {
    pub fn generate(schema: &Schema, entities: &[EntityMetadata]) -> String {
        let mut sdl = String::new();

        // Federation 2.5 link directive
        sdl.push_str("extend schema\n");
        sdl.push_str("  @link(url: \"https://specs.apollo.dev/federation/v2.5\")\n\n");

        // Federation types
        sdl.push_str(&Self::federation_types());

        // Entity types with @key directives
        for entity in entities {
            sdl.push_str(&Self::format_entity(entity));
        }

        sdl
    }

    fn federation_types() -> String {
        r#"scalar _Any
union _Entity = User | Post | Product  # Dynamic based on entities

type _Service {
  sdl: String!
}

extend type Query {
  _service: _Service!
  _entities(representations: [_Any!]!): [_Entity]!
}
"#.to_string()
    }

    fn format_entity(entity: &EntityMetadata) -> String {
        let mut sdl = format!(
            "type {} @key(fields: \"{}\")",
            entity.type_name,
            entity.key_field.join(" ")
        );

        // Add fields
        sdl.push_str(" {\n");

        for (field_name, field_type) in &entity.fields {
            sdl.push_str(&format!("  {}: {}\n", field_name, field_type));
        }

        sdl.push_str("}\n\n");
        sdl
    }
}
```

**Testing**:
```python
# tests/federation/test_sdl.py
def test_sdl_generation():
    @entity
    class User:
        id: str
        name: str

    @entity
    class Post:
        id: str
        title: str

    sdl = generate_federation_sdl()

    assert "@link" in sdl
    assert "scalar _Any" in sdl
    assert "_service" in sdl
    assert "_entities" in sdl
    assert "@key(fields: \"id\")" in sdl

def test_sdl_with_extensions():
    @extend_entity(key="id")
    class Product:
        id: str = external()
        reviews: list["Review"]

    sdl = generate_federation_sdl()

    assert "extend type Product" in sdl
    assert "@external" in sdl
```

---

#### Day 3-4: `_service` Query (8-10 hours)

**Implementation**:

```python
# src/fraiseql/federation/service.py
from typing import Optional
from functools import lru_cache

class ServiceResolver:
    """Auto-cached _service resolver."""

    _cached_sdl: Optional[str] = None
    _cache_timestamp: float = 0.0
    _cache_ttl: int = 3600  # 1 hour

    @classmethod
    def resolve(cls) -> str:
        """Resolve _service query (returns SDL)."""
        import time

        now = time.time()

        # Check cache validity
        if cls._cached_sdl and (now - cls._cache_timestamp) < cls._cache_ttl:
            return cls._cached_sdl

        # Regenerate SDL
        from .sdl_generator import generate_federation_sdl
        cls._cached_sdl = generate_federation_sdl()
        cls._cache_timestamp = now

        return cls._cached_sdl
```

**Performance Target**: < 0.1ms (cached response)

---

#### Day 5: Apollo Router Integration (8-10 hours)

**Objective**: Test with real Apollo Router

**Tasks**:
1. Set up Apollo Router locally
2. Configure FraiseQL as subgraph
3. Test entity resolution
4. Test cross-subgraph queries

**Test scenarios**:
```graphql
# Simple entity fetch
query {
  user(id: "123") {
    id
    name
  }
}

# Cross-subgraph reference
query {
  user(id: "123") {
    id
    posts {  # From Posts subgraph
      title
    }
  }
}
```

---

### Week 4: Batch Loading & Performance (30-40 hours)

#### Day 1-3: DataLoader Implementation (12-15 hours)

**Files to Create**:
- `fraiseql_rs/src/federation/batch_loader.rs` - Batch loading

**Implementation**:

```rust
// fraiseql_rs/src/federation/batch_loader.rs
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

pub struct EntityBatchLoader {
    cache: Arc<DashMap<String, Entity>>,
    batch_window: Duration,
    batch_size: usize,
}

impl EntityBatchLoader {
    pub fn new(batch_window_ms: u64, batch_size: usize) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            batch_window: Duration::from_millis(batch_window_ms),
            batch_size,
        }
    }

    pub async fn load_many(
        &self,
        entity_type: &str,
        keys: Vec<String>,
        db_pool: &Pool,
    ) -> Vec<Option<Entity>> {
        // 1. Check cache for each key
        let mut cached = Vec::new();
        let mut uncached_keys = Vec::new();
        let mut uncached_indices = Vec::new();

        for (i, key) in keys.iter().enumerate() {
            if let Some(entity) = self.cache.get(&format!("{}:{}", entity_type, key)) {
                cached.push(Some(entity.clone()));
            } else {
                uncached_keys.push(key.clone());
                uncached_indices.push(i);
                cached.push(None);
            }
        }

        // 2. If all cached, return immediately
        if uncached_keys.is_empty() {
            return cached;
        }

        // 3. Wait for batch window to collect more requests
        sleep(self.batch_window).await;

        // 4. Execute batch query
        let query = format!(
            "SELECT * FROM {} WHERE id = ANY($1)",
            entity_type
        );

        let mut conn = db_pool.get().await.unwrap();
        let rows = conn.query(&query, &[&uncached_keys]).await.unwrap();

        // 5. Cache results
        for row in rows {
            let entity = Entity::from_row(&row);
            self.cache.insert(
                format!("{}:{}", entity_type, entity.id.clone()),
                entity,
            );
        }

        // 6. Return all results
        let mut results = cached;

        for idx in uncached_indices {
            if let Some(entity) = self.cache.get(&format!("{}:{}", entity_type, keys[idx])) {
                results[idx] = Some(entity.clone());
            }
        }

        results
    }
}
```

**Performance Target**: < 50ms for 100 entities

---

#### Day 4-5: Performance Optimization (8-10 hours)

**Optimizations**:
1. Connection pooling (reuse connections)
2. Query preparation (pre-compiled)
3. Memory pooling (Arc/Weak)
4. Zero-copy where possible

**Benchmarks**:
```rust
#[bench]
fn bench_entity_resolution(b: &mut Bencher) {
    // Target: < 2ms for single entity
}

#[bench]
fn bench_batch_resolution(b: &mut Bencher) {
    // Target: < 50ms for 100 entities
}
```

---

### Week 5: Python API Polish & Presets (20-30 hours)

#### Day 1-2: Schema Configuration (6-8 hours)

**Python API**:

```python
# src/fraiseql/federation/__init__.py
from fraiseql import Schema

# SIMPLE: Enable federation (auto-detects entities)
schema = Schema(federation=True)

# STANDARD: With options
schema = Schema(
    federation=FederationConfig(
        version="2.5",
        auto_keys=True,
    )
)

# ADVANCED: With presets
schema = Schema(federation=Presets.STANDARD)
```

---

#### Day 3: Presets (6-8 hours)

**Implementation**: Already planned in config.py above

---

#### Day 4-5: Documentation (6-8 hours)

**Documentation Structure**:
1. **Quick Start** (5 min) - Federation Lite
2. **Type Extensions** - Referencing external entities
3. **Computed Fields** - Using `@requires`
4. **Gateway Setup** - Apollo Router configuration
5. **Advanced** - All directives (Phase 17b)

**Examples**:
```python
# examples/federation/01_lite.py
from fraiseql import Schema, entity

@entity
class User:
    id: str
    name: str

schema = Schema(federation=True)

# examples/federation/02_standard.py
from fraiseql import Schema, entity, extend_entity, external

@entity
class User:
    id: str
    name: str

@extend_entity(key="id")
class Product:
    id: str = external()
    reviews: list["Review"]

schema = Schema(federation=Presets.STANDARD)
```

---

### Week 6: Testing & Production (15-20 hours)

#### Day 1-3: Comprehensive Testing (9-12 hours)

**Test Categories**:
1. Unit tests - Auto-key detection, directive parsing
2. Integration tests - Entity resolution, SDL generation
3. Gateway tests - Apollo Router composition
4. Performance tests - Benchmarks

**Test Coverage Target**: 90%+

---

#### Day 4-5: Migration Guide & Rollout (6-8 hours)

**Migration Path**:
```python
# BEFORE: Custom subgraph
class User:
    id: str
    name: str

# AFTER: Federation Lite
@entity  # That's it!
class User:
    id: str
    name: str

schema = Schema(federation=True)
```

---

## 🎯 Success Metrics

### Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Entity resolution | < 2ms | Benchmark |
| Batch resolution (100) | < 50ms | Load test |
| `_service` query | < 0.1ms | Prometheus |
| SDL generation | < 10ms | Benchmark |
| Auto-key detection | < 0.1ms | Unit test |

### Simplicity Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Lines to enable | 1 | `@entity` |
| Required config | 0 | Auto-detect all |
| Learning time (Lite) | 5 min | Tutorial |
| Learning time (Standard) | 30 min | User guide |

---

## 📁 File Structure

```
fraiseql_rs/src/federation/
├── mod.rs                      # Module exports
├── auto_detect.rs              # ⭐ Auto-key detection
├── lite.rs                     # ⭐ Federation Lite mode
├── directives.rs               # Core directives
├── standard.rs                 # ⭐ Standard mode
├── entities_resolver.rs        # Auto-generated resolver
├── extensions.rs               # Type extensions
├── requires.rs                 # @requires directive
├── provides.rs                 # @provides directive
├── batch_loader.rs             # Auto-batching
├── sdl_generator.rs            # Auto-SDL generation
├── service_resolver.rs         # _service query
└── py_bindings.rs              # Python interface

src/fraiseql/federation/
├── __init__.py                 # ⭐ Lite API exports
├── decorators.py               # @entity, @extend_entity
├── auto_detect.py              # Python auto-detection
├── config.py                   # FederationConfig, Presets
├── entities.py                 # EntitiesResolver
├── service.py                  # ServiceResolver
└── schema.py                   # Schema integration

examples/federation/
├── 01_lite.py                  # ⭐ Auto-key detection
├── 02_standard.py              # Type extensions
├── 03_computed_fields.py       # @requires/@provides
├── 04_gateway_setup.py         # Apollo Router config
└── 05_migration.py             # Migration guide

tests/federation/
├── test_auto_detect.py
├── test_decorators.py
├── test_entities.py
├── test_directives.py
├── test_sdl.py
├── test_service.py
├── test_extensions.py
├── test_requires.py
├── test_batch_loader.py
├── test_performance.py
└── test_gateway.py

docs/federation/
├── quickstart.md                # ⭐ 5-minute Lite tutorial
├── type-extensions.md           # Standard mode
├── computed-fields.md           # @requires/@provides
├── gateway-setup.md             # Apollo Router
├── performance.md               # Optimization guide
└── advanced.md                  # Phase 17b features
```

---

## 🚀 Implementation Strategy

### Phase 1: Foundation (Week 1)
1. Implement auto-key detection (Rust + Python)
2. Create `@entity` decorator with registry
3. Auto-generate `_entities` resolver
4. Basic testing

**Deliverable**: Federation Lite MVP

### Phase 2: Extensions (Week 2)
1. Directive parsing (4 core directives)
2. Type extensions with `@external`
3. `@requires` and `@provides`
4. Integration testing

**Deliverable**: Federation Standard support

### Phase 3: Integration (Week 3)
1. SDL generation with Federation 2.5 link
2. `_service` query (cached)
3. Apollo Router integration tests
4. Gateway composition verification

**Deliverable**: Production-ready gateway support

### Phase 4: Performance (Week 4)
1. DataLoader batch loading
2. Performance optimization
3. Benchmarking
4. Load testing

**Deliverable**: < 2ms entity resolution, < 50ms batch

### Phase 5: Polish (Week 5)
1. Schema configuration API
2. Presets (LITE, STANDARD, ADVANCED)
3. Documentation (5 examples + guides)
4. User guide

**Deliverable**: Production-ready API + documentation

### Phase 6: Production (Week 6)
1. Comprehensive testing (90%+ coverage)
2. Migration guide
3. Rollout plan
4. Final verification

**Deliverable**: Production release ready

---

## 🔄 Integration with Existing FraiseQL

### Schema Builder Integration

```python
# src/fraiseql/gql/schema_builder.py (modify)
def build_fraiseql_schema(
    classes: List[type],
    federation: Union[bool, FederationConfig, Presets] = False,
) -> GraphQLSchema:
    """Build schema with optional federation support."""

    # ... existing schema building ...

    # Optionally add federation layer
    if federation:
        from .federation import add_federation_support
        schema = add_federation_support(schema, federation)

    return schema
```

### Rust Integration Points

1. **PyO3 bindings**: Expose federation functions to Python
2. **Existing pipeline**: Reuse query execution pipeline
3. **Response builder**: Integrate with existing response construction

---

## 📊 Risk Mitigation

### Risk: Complexity in auto-key detection
**Mitigation**: Clear error messages, fallback to explicit key

### Risk: Performance degradation
**Mitigation**: Benchmarks in every week, performance gates in CI

### Risk: Gateway incompatibility
**Mitigation**: Test with real Apollo Router early (Week 3)

### Risk: N+1 queries
**Mitigation**: DataLoader pattern implementation (Week 4)

---

## 📞 Approval Gate

Before proceeding to implementation, confirm:
- [ ] Architecture approved
- [ ] File structure approved
- [ ] Performance targets confirmed
- [ ] Test strategy approved
- [ ] Timeline realistic

---

**Next Step**: Begin Week 1 implementation (auto-key detection + @entity decorator)
