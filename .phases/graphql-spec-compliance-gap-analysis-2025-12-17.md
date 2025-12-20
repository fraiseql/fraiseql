# FraiseQL GraphQL Specification Compliance Gap Analysis

**Date:** December 17, 2025
**Timestamp:** 2025-12-17T11:15:00Z
**Version:** v1.8.5
**Status:** Analysis Complete - Ready for Implementation Planning

---

## Executive Summary

FraiseQL implements a **production-ready GraphQL execution engine** with **~85-90% specification compliance**. This document identifies remaining specification gaps and prioritizes them by implementation effort and business impact.

**Key Findings:**
- 11 major GraphQL features fully compliant
- 5 features partially implemented
- 8 features intentionally omitted (with documented trade-offs)
- **5 easy-to-implement gaps identified** (2-8 hours each)
- Recent implementation surge: 16 commits in v1.8.5 adding multi-field query support

---

## Part 1: Current Compliance Status

### Fully Implemented Features (100% Coverage)

| Feature | Status | Evidence |
|---------|--------|----------|
| Query Operations | ✅ | Unified executor with multi-field optimization |
| Mutation Operations | ✅ | Declarative SQL generation, Success/Error types |
| Subscription Operations | ✅ | WebSocket with graphql-ws & graphql-transport-ws |
| Scalar Types (60+) | ✅ | Date, UUID, Network, Financial, Location types |
| Object Types | ✅ | @fraise_type decorator with computed fields |
| Input Object Types | ✅ | @fraise_input with full validation |
| Interface Types | ✅ | @fraise_interface with implementation tracking |
| Union Types | ✅ | FraiseUnion annotation with __typename resolution |
| Enum Types | ✅ | @fraise_enum with value mapping |
| List & NonNull Types | ✅ | Python 3.10+ syntax (list[T], T \| None) |
| Field Resolution | ✅ | Async support, computed fields, custom resolvers |
| Arguments & Values | ✅ | Literals, variables, lists, objects, enums |
| Field Aliases | ✅ | Full support including multi-field queries |
| @skip & @include | ✅ | With variable support, proper precedence |
| @deprecated | ✅ | Schema-level field deprecation |
| @specifiedBy | ✅ | Custom scalar specification URLs |
| Schema Introspection | ✅ | Full __schema query with policy control |
| Type Introspection | ✅ | __type query with field recursion |
| Document Validation | ✅ | Full graphql-core validation rules |
| Argument Validation | ✅ | Type checking, null validation, coercion |
| Error Formatting | ✅ | Spec-compliant with locations (line/column) |
| Error Locations | ✅ | 1-indexed line/column from AST offsets |

**Key Implementation Details:**

```python
# Error Location Reporting (routers.py:210-244)
def _extract_field_location(field_node: Any) -> dict[str, int] | None:
    """Convert AST offset to 1-indexed line/column per GraphQL spec"""
    # Used in multi-field error collection (routers.py:808-819)

# Multi-field Query Support (routers.py:590-843)
# Phase 1-5 implementation in v1.8.5:
# - Route detection (routers.py:1140)
# - Field extraction with aliases (routers.py:544-549)
# - Fragment expansion (routers.py:299-455)
# - Variable handling (routers.py:247-296)
# - Directive evaluation (routers.py:78-144)
# - Argument extraction (routers.py:640-646)
# - Rust merge pipeline (fraiseql_rs/src/)
```

---

### Partially Implemented Features (50-75% Coverage)

#### 1. Fragment Spreads in Nested Selections

**Current Status:** Root-level only

```
Working (root-level):
  query {
    ...UserData              # ✅ Works
    posts { id }
  }

NOT working (nested):
  query {
    users {
      ...userFields          # ❌ Not expanded
    }
  }
```

**Implementation Gap:**
- `_extract_root_query_fields()` handles root fragments only (lines 518-522)
- Fragment resolver exists but not called recursively
- Rust pipeline receives flat field list

**Scope:**
- Lines: ~50 LOC changes
- Files: routers.py (1), fragment_resolver.py (2)
- Tests: 5 new test cases

---

#### 2. DataLoader / Batching

**Current Status:** Per-request only, manual registration

```python
# Current: Manual integration
class UserDataLoader(DataLoader[UUID, dict]):
    async def batch_load(self, ids):
        return await fetch_users_batch(ids)

# Auto-integration missing:
# - No per-request context scoping
# - No field resolver wrapping
# - No batch size hints
```

**Implementation Gap:**
- DataLoader exists (optimization/dataloader.py)
- Registry pattern missing
- Context management needed
- Auto-discovery not implemented

**Scope:**
- Lines: ~150 LOC
- Files: registry.py (1), dependencies.py (2), resolver_wrappers.py (3)
- Tests: 12-15 integration tests

---

#### 3. Custom Directives

**Current Status:** Only built-in directives, limited RBAC

```python
# Built-in: @skip, @include, @deprecated, @specifiedBy
# RBAC: @requires_permission, @requires_role

# Missing: Business logic directives
# @rate_limit(calls: Int!, window: String!)      # Not implemented
# @access_level(minLevel: Int!)                   # Not implemented
# @cache(ttl: Int!)                               # Not implemented
# @validate(pattern: String!)                     # Not implemented
```

**Implementation Gap:**
- Directive location support limited to FIELD_DEFINITION
- No custom directive middleware
- RBAC directives exist but not generalized

**Scope:**
- Lines: ~100 LOC
- Files: enterprise/rbac/directives.py (1), routers.py (2), schema_builder.py (3)
- Tests: 8-10 directive tests

---

#### 4. WebSocket Subscriptions

**Current Status:** WebSocket only, no HTTP SSE

```
Working: WebSocket
  - graphql-ws protocol ✅
  - graphql-transport-ws protocol ✅
  - AsyncGenerator-based execution ✅

NOT working: HTTP Server-Sent Events
  - No @stream/@defer directives ❌
  - No incremental delivery protocol ❌
```

**Implementation Gap:**
- Streaming infrastructure exists (subscriptions)
- SSE response format not implemented
- Incremental delivery protocol missing
- @stream/@defer directives not defined

**Scope:**
- Lines: ~150 LOC
- Files: routers.py (1), execute.py (2), schema_builder.py (3)
- Tests: 8-10 streaming tests

---

#### 5. Fragment Support Edge Cases

**Current Status:** Basic fragments work, but validation missing

```python
# Missing validations:
fragment A on User { name ...B }  # Cycle detection ❌
fragment B on User { email ...A }

fragment StrictTypes on User { id }
query { users { ...StrictTypes } }  # Type check ❌
```

**Implementation Gap:**
- Fragment cycle detection not implemented
- Type compatibility validation missing
- Fragment usage statistics not collected
- Complexity analyzer simplified (line 185 in query_complexity.py)

**Scope:**
- Lines: ~100 LOC
- Files: fragment_resolver.py (1), query_complexity.py (2)
- Tests: 10 edge case tests

---

### Not Implemented Features (0% Coverage - Intentional)

#### 1. **Nested Error Recovery** ⚠️ Architectural Decision

**Status:** Intentionally NOT implemented

**Rationale:** Lines 461-480 in routers.py:
```python
def _check_nested_errors(data: Any, path: list[str | int]) -> list[dict]:
    """
    NOTE: This function is not implemented due to FraiseQL architectural constraints.
    FraiseQL uses database views and table views that don't support partial failures.
    When a nested resolver fails, the entire parent field must fail to maintain
    data consistency with the underlying database views.
    """
```

**GraphQL Spec Allows:** Partial results
```graphql
{
  users {
    id         # ✅ Returns successfully
    profile {  # ❌ Fails, but user field continues
      title
    }
  }
}
# Result: { data: { users: [{ id: 1, profile: null }] }, errors: [...] }
```

**FraiseQL Implementation:**
```graphql
# Same query
# Result: { data: { users: null }, errors: [...] }
# Reason: Profile is a nested field that failed
```

**Trade-off Analysis:**
- ✅ Guarantees data consistency with database views
- ✅ Simpler error handling (fail-fast)
- ❌ Less granular error information
- ❌ Not spec-compliant for nested errors

**Workaround:** Split into separate queries
```graphql
# Instead of:
{ users { id profile { title } } }

# Use:
query Users { users { id profileId } }
query UserProfiles { userProfiles(ids: [...]) { title } }
```

---

#### 2. **@stream & @defer Directives**

**Status:** Not implemented

**What's Missing:**
- Incremental field streaming
- Deferred field resolution
- Server-Sent Events (SSE) support

**Why Hard:**
- Would require streaming infrastructure
- Incremental execution model change
- Protocol complexity

**Workaround:** Use pagination
```graphql
# Instead of:
{ items @stream(initialCount: 10) { id } }

# Use:
{ items(first: 10, after: null) { edges { node { id } } } }
```

---

#### 3. **GraphQL Federation / Apollo Federation**

**Status:** Not implemented

**Why:** Single-service architecture doesn't need federation

**Workaround:** Use schema stitching or separate services with client-side composition

---

#### 4. **HTTP Server-Sent Events (SSE)**

**Status:** Not implemented, WebSocket only

**Why:** WebSocket already provides better performance and reliability

**Workaround:** Use WebSocket for subscriptions

---

#### 5. **Fragment Spreads as Standalone Operations**

**Status:** Partially missing

Currently fragments must be used within operations. Fragment-only queries not supported.

---

#### 6. **Schema Directives & Object Type Directives**

**Status:** Limited support

- FIELD_DEFINITION: ✅ Full support
- SCHEMA: ⚠️ Limited
- OBJECT: ⚠️ Limited
- Others: ❌ Not supported

---

## Part 2: Easy-to-Implement Gaps (2-8 hours)

### Priority Ranking Matrix

```
Complexity vs Impact:

            HIGH IMPACT
                 ↑
                 │
    ⭐⭐⭐⭐⭐  │  [1] Nested Fragments        [3] Auto-DataLoader  ⭐⭐⭐⭐⭐
               │      (2-3h, High Impact)    (4-6h, High Impact)
               │
    ⭐⭐⭐⭐   │  [2] Directives             [4] HTTP Streaming
               │      (2-4h)                 (6-8h)
               │
    ⭐⭐⭐    │  [5] Fragment Cycles
               │      (3-4h)
               │
    ⭐⭐     │
               │
    ⭐      └─────────────────────────────────
              EASY                      HARD
```

---

### Gap #1: Nested Field Fragments ⭐⭐⭐⭐⭐ (Priority 1)

**Complexity:** 1 (Trivial)
**Effort:** 2-3 hours
**Impact:** High
**ROI:** Excellent

#### Current State
- Fragment spreads expanded at root level only
- `_expand_fragment_spread()` exists and works (lines 299-387)
- Nested fragments not processed

#### Implementation Plan

**Step 1:** Extend Fragment Resolution (30 min)
```python
# File: fraiseql/fastapi/routers.py
# Location: Modify _extract_root_query_fields()

# Current (lines 518-522):
for selection in selection_set.selections:
    if isinstance(selection, FragmentSpreadNode):
        expanded = _expand_fragment_spread(selection, document, variables)
        # Only at root level

# Change to:
def process_selections(selections, document, variables):
    for selection in selections:
        if isinstance(selection, FieldNode):
            # Recursively process nested selections
            if selection.selection_set:
                selection.selection_set.selections = process_selections(
                    selection.selection_set.selections, document, variables
                )
        elif isinstance(selection, FragmentSpreadNode):
            # Expand fragment
            expanded = _expand_fragment_spread(selection, document, variables)
            # Recursively process expanded selections
            expanded = process_selections(expanded, document, variables)
    return selections
```

**Step 2:** Update Field Extraction (30 min)
```python
# Apply recursive processing to nested selections
# Ensure field names, aliases, and arguments preserved through recursion
```

**Step 3:** Add Tests (1 hour)
```python
# tests/unit/fastapi/test_multi_field_fragments.py

def test_nested_fragment_spread():
    """Fragment spread in nested selection"""
    query = """
    fragment UserFields on User {
        id
        name
    }

    query {
        users {
            ...UserFields
            email
        }
    }
    """
    # Should expand UserFields within users field

def test_deeply_nested_fragments():
    """Multiple levels of nested fragments"""

def test_nested_fragment_with_alias():
    """Fragment in nested selection with alias"""

def test_mixed_fragments_inline_and_spread():
    """Mix of inline and spread fragments in nested"""
```

**Dependencies:** None (fragment resolver already works)

**Risk Level:** Low (extending existing pattern)

---

### Gap #2: Custom Business Logic Directives ⭐⭐⭐⭐⭐ (Priority 2)

**Complexity:** 2 (Easy-Moderate)
**Effort:** 2-4 hours
**Impact:** High (Security, Performance, Validation)
**ROI:** Excellent

#### Current State
- @skip, @include, @deprecated working
- RBAC directives exist but not generalized
- No framework for custom directives

#### Implementation Plan

**Step 1:** Create Directive Framework (45 min)
```python
# File: fraiseql/fastapi/directives.py (NEW)

from abc import ABC, abstractmethod
from typing import Any

class CustomDirective(ABC):
    """Base class for custom GraphQL directives"""

    @abstractmethod
    async def evaluate(
        self,
        field_value: Any,
        directive_args: dict[str, Any],
        context: dict,
    ) -> Any:
        """Evaluate directive and return transformed value"""

class RateLimitDirective(CustomDirective):
    """@rate_limit(calls: Int!, window: String!) directive"""

    async def evaluate(self, field_value, args, context):
        # Implement rate limiting
        calls = args.get("calls")
        window = args.get("window")  # "minute", "hour", "day"
        # Check rate limit, raise if exceeded
        return field_value

class AccessLevelDirective(CustomDirective):
    """@access_level(minLevel: Int!) directive"""

    async def evaluate(self, field_value, args, context):
        # Check user access level
        min_level = args.get("minLevel")
        user_level = context.get("user", {}).get("access_level", 0)
        if user_level < min_level:
            raise PermissionError(f"Requires access level {min_level}")
        return field_value

class CacheDirective(CustomDirective):
    """@cache(ttl: Int!) directive"""

    async def evaluate(self, field_value, args, context):
        # Apply caching
        ttl = args.get("ttl")
        # Cache field_value for ttl seconds
        return field_value
```

**Step 2:** Add Directive Registration (30 min)
```python
# File: fraiseql/gql/schema_builder.py

class DirectiveRegistry:
    _directives = {}

    @classmethod
    def register(cls, name: str, directive: CustomDirective):
        cls._directives[name] = directive

    @classmethod
    def get(cls, name: str):
        return cls._directives.get(name)

# Register directives
DirectiveRegistry.register("rate_limit", RateLimitDirective())
DirectiveRegistry.register("access_level", AccessLevelDirective())
DirectiveRegistry.register("cache", CacheDirective())
```

**Step 3:** Integrate with Resolver Pipeline (45 min)
```python
# File: fraiseql/fastapi/routers.py
# Modify execute resolver section (around line 750)

# After resolver executes:
result = await resolver(None, info, **field_args)

# Apply directives:
if field_node.directives:
    for directive in field_node.directives:
        directive_impl = DirectiveRegistry.get(directive.name.value)
        if directive_impl:
            args = {
                arg.name.value: evaluate_argument_value(arg.value, variables)
                for arg in directive.arguments or []
            }
            result = await directive_impl.evaluate(result, args, context)
```

**Step 4:** Define Directive Schema (30 min)
```python
# File: fraiseql/gql/schema_builder.py

# Add directive definitions to schema
schema_directives = [
    GraphQLDirective(
        name="rate_limit",
        locations=[DirectiveLocation.FIELD_DEFINITION],
        args={
            "calls": GraphQLArgument(GraphQLNonNull(GraphQLInt)),
            "window": GraphQLArgument(GraphQLNonNull(GraphQLString)),
        },
    ),
    # Similar for @access_level, @cache
]
```

**Step 5:** Add Tests (1 hour)
```python
# tests/unit/fastapi/test_custom_directives.py

def test_rate_limit_directive():
    """@rate_limit directive enforcement"""

def test_access_level_directive():
    """@access_level directive checks user permission"""

def test_cache_directive():
    """@cache directive applies caching"""

def test_multiple_directives():
    """Multiple directives on same field"""

def test_directive_with_variables():
    """Directive arguments can use variables"""
```

**Dependencies:** RBAC directive module exists (enterprise/rbac/directives.py)

**Risk Level:** Low (extends existing pattern)

---

### Gap #3: Auto-integrated DataLoaders ⭐⭐⭐⭐⭐ (Priority 3)

**Complexity:** 2.5 (Moderate)
**Effort:** 4-6 hours
**Impact:** High (Automatic N+1 prevention)
**ROI:** Excellent (Dev productivity)

#### Current State
- DataLoader class fully implemented
- Loaders exist: UserLoader, ProjectLoader, etc.
- Manual registration required
- Per-request context scoping missing

#### Implementation Plan

**Step 1:** Create Loader Registry (1 hour)
```python
# File: fraiseql/optimization/loader_registry.py (NEW)

class LoaderRegistry:
    """Registry for auto-discovering and instantiating loaders"""

    _loaders: dict[str, type] = {}
    _per_request: dict[str, Any] = {}

    @classmethod
    def register(cls, name: str, loader_class: type):
        cls._loaders[name] = loader_class

    @classmethod
    def auto_discover(cls):
        """Auto-discover loaders from optimization/loaders.py"""
        # Import all classes from loaders.py
        # Filter by DataLoader subclass
        # Register automatically

    @classmethod
    def create_context_loaders(cls, db_connection, request_id):
        """Create per-request loader instances"""
        context_loaders = {}
        for name, loader_class in cls._loaders.items():
            context_loaders[name] = loader_class(db_connection)
        cls._per_request[request_id] = context_loaders
        return context_loaders

    @classmethod
    def cleanup_context_loaders(cls, request_id):
        """Clean up per-request loaders after request"""
        if request_id in cls._per_request:
            del cls._per_request[request_id]
```

**Step 2:** Integrate with Context Creation (1 hour)
```python
# File: fraiseql/fastapi/dependencies.py
# Modify build_graphql_context()

def build_graphql_context(
    request: Request,
    db_connection: ...
) -> dict:
    request_id = str(uuid4())

    # Create per-request loaders
    loaders = LoaderRegistry.create_context_loaders(db_connection, request_id)

    context = {
        "request": request,
        "db": db_connection,
        "loaders": loaders,
        "request_id": request_id,
    }

    return context
```

**Step 3:** Wrap Resolvers with Loader Injection (1 hour)
```python
# File: fraiseql/gql/resolver_wrappers.py

def inject_loader_wrapper(resolver_func, loader_name):
    """Wrap resolver to inject appropriate loader"""

    @wraps(resolver_func)
    async def wrapped(root, info, **kwargs):
        # Get loader from context
        loaders = info.context.get("loaders", {})
        loader = loaders.get(loader_name)

        if loader:
            # Add loader to context for resolver to use
            info.context["active_loader"] = loader

        return await resolver_func(root, info, **kwargs)

    return wrapped
```

**Step 4:** Update Field Resolution (1 hour)
```python
# File: fraiseql/gql/schema_builder.py
# Modify field resolver creation

# When building field with foreign key:
if is_foreign_key_field:
    loader_name = f"{type_name.lower()}_{field_name}_loader"
    wrapped_resolver = inject_loader_wrapper(resolver, loader_name)
    # Use wrapped_resolver
```

**Step 5:** Add Cleanup in FastAPI Middleware (30 min)
```python
# File: fraiseql/fastapi/middleware.py (or routers.py)

@app.middleware("http")
async def cleanup_loaders(request: Request, call_next):
    response = await call_next(request)

    # Extract request_id from context
    request_id = getattr(request.state, "request_id", None)
    if request_id:
        LoaderRegistry.cleanup_context_loaders(request_id)

    return response
```

**Step 6:** Add Tests (1.5 hours)
```python
# tests/integration/performance/test_auto_dataloader.py

def test_dataloader_auto_instantiation():
    """Loaders auto-created per request"""

def test_loader_injection_in_resolver():
    """Resolver receives injected loader"""

def test_no_n_plus_one_with_auto_loaders():
    """Auto loaders prevent N+1 queries"""

def test_per_request_loader_isolation():
    """Each request has isolated loader instances"""

def test_loader_cleanup_after_request():
    """Loaders cleaned up after request completes"""
```

**Dependencies:** DataLoader exists, fully functional

**Risk Level:** Low (non-intrusive injection)

---

### Gap #4: HTTP Streaming / @stream Support ⭐⭐ (Priority 4)

**Complexity:** 2.5 (Moderate)
**Effort:** 6-8 hours
**Impact:** Medium (Advanced UX)
**ROI:** Good (not critical)

#### Current State
- WebSocket subscriptions fully work
- AsyncGenerator pattern established
- Streaming infrastructure partially in place

#### Implementation Plan

**Step 1:** Define @stream & @defer Directives (1 hour)
```python
# File: fraiseql/gql/schema_builder.py

stream_directive = GraphQLDirective(
    name="stream",
    locations=[DirectiveLocation.FIELD],
    args={
        "initialCount": GraphQLArgument(GraphQLInt, default_value=0),
        "label": GraphQLArgument(GraphQLString),
    },
)

defer_directive = GraphQLDirective(
    name="defer",
    locations=[DirectiveLocation.FIELD, DirectiveLocation.FRAGMENT_SPREAD],
    args={
        "label": GraphQLArgument(GraphQLString),
    },
)
```

**Step 2:** Implement Incremental Delivery Protocol (2 hours)
```python
# File: fraiseql/graphql/incremental_delivery.py (NEW)

class IncrementalDelivery:
    """Implements GraphQL incremental delivery protocol"""

    @staticmethod
    def create_response(data, errors=None, incremental=None):
        """Create response per spec"""
        response = {}
        if data is not None:
            response["data"] = data
        if errors:
            response["errors"] = errors
        if incremental:
            response["incremental"] = incremental
        return response

    @staticmethod
    async def stream_responses(async_gen):
        """Convert async generator to SSE stream"""
        async for response in async_gen:
            yield f"data: {json.dumps(response)}\n\n"
```

**Step 3:** Modify Execution for Streaming (2 hours)
```python
# File: fraiseql/graphql/execute.py
# Modify execute_graphql()

async def execute_graphql_streaming(query, variables, operation_name):
    """Execute query with streaming support"""

    document = parse(query)
    operation = get_operation(document, operation_name)

    # Check for @stream/@defer directives
    if has_streaming_directives(operation):
        # Return async generator
        return stream_execution(document, variables, operation_name)
    else:
        # Normal execution
        return await execute_graphql(...)

async def stream_execution(document, variables, operation_name):
    """Execute with streaming, yield incremental results"""

    # Initial execution
    initial_result = await execute_graphql(
        document, variables, operation_name,
        defer_stream_directives=False  # Skip @stream/@defer fields
    )

    yield IncrementalDelivery.create_response(
        data=initial_result.get("data"),
        errors=initial_result.get("errors"),
    )

    # Stream deferred fields
    for deferred_field in get_deferred_fields(document):
        result = await execute_field(deferred_field)
        yield IncrementalDelivery.create_response(
            incremental=[{
                "path": deferred_field["path"],
                "data": result,
            }]
        )
```

**Step 4:** Add HTTP Streaming Response (1 hour)
```python
# File: fraiseql/fastapi/routers.py
# Modify graphql_endpoint()

@router.post("/graphql")
async def graphql_endpoint(request: Request):
    # Check for streaming query
    body = await request.json()
    query = body.get("query")

    if should_stream(query):
        # Return streaming response
        async def response_generator():
            async for chunk in execute_graphql_streaming(query, ...):
                yield chunk

        return StreamingResponse(
            response_generator(),
            media_type="text/event-stream",
            headers={
                "Cache-Control": "no-cache",
                "Connection": "keep-alive",
            }
        )
    else:
        # Normal JSON response
        return await normal_execution(...)
```

**Step 5:** Add Tests (2 hours)
```python
# tests/integration/graphql/test_streaming.py

async def test_stream_directive_basic():
    """@stream directive returns initial items then incremental"""

async def test_defer_directive_basic():
    """@defer directive defers field resolution"""

async def test_mixed_stream_and_defer():
    """@stream and @defer used together"""

async def test_streaming_with_errors():
    """Errors during streaming incremental response"""

async def test_stream_with_variables():
    """@stream with variable arguments"""
```

**Dependencies:** Subscriptions already use AsyncGenerator pattern

**Risk Level:** Medium (new protocol, well-specified)

---

### Gap #5: Fragment Cycle Detection ⭐ (Priority 5)

**Complexity:** 2 (Easy-Moderate)
**Effort:** 3-4 hours
**Impact:** Medium (Stability, DoS prevention)
**ROI:** Good

#### Current State
- Fragment resolution works
- No cycle detection
- No type validation

#### Implementation Plan

**Step 1:** Add Cycle Detection (1 hour)
```python
# File: fraiseql/core/fragment_resolver.py
# Modify resolve_all_fields()

def resolve_all_fields(
    selection_set,
    document,
    visited_fragments=None,
):
    """Resolve fields with cycle detection"""
    if visited_fragments is None:
        visited_fragments = set()

    fields = []

    for selection in selection_set.selections:
        if isinstance(selection, FragmentSpreadNode):
            fragment_name = selection.name.value

            # Check for cycle
            if fragment_name in visited_fragments:
                raise ValueError(f"Circular fragment reference: {fragment_name}")

            # Add to visited
            visited = visited_fragments | {fragment_name}

            # Get fragment definition
            fragment = document.definitions.get(fragment_name)

            # Recursively resolve with cycle detection
            fragment_fields = resolve_all_fields(
                fragment.selection_set,
                document,
                visited,
            )
            fields.extend(fragment_fields)
```

**Step 2:** Add Type Validation (1 hour)
```python
# Validate fragment type matches field type

def validate_fragment_type(fragment_def, field_type, schema):
    """Ensure fragment type is compatible with field type"""

    fragment_type_name = fragment_def.type_condition.name.value
    fragment_type = schema.type_map.get(fragment_type_name)

    # Check if fragment type is valid for field type
    if not is_type_compatible(fragment_type, field_type):
        raise ValueError(
            f"Fragment {fragment_def.name.value} of type {fragment_type_name} "
            f"cannot be applied to field of type {field_type}"
        )
```

**Step 3:** Update Complexity Analyzer (1 hour)
```python
# File: fraiseql/analysis/query_complexity.py
# Fix line 185-186 simplification

# Before:
# Simplified - we'd properly handle recursive fragments

# After:
def enter_fragment_spread(self, node, *_args):
    fragment_name = node.name.value

    # Get fragment definition
    fragment = self.fragments.get(fragment_name)
    if not fragment:
        return

    # Mark as visited
    self.visited_fragments.add(fragment_name)

    # Analyze fragment complexity
    self.visit(fragment.selection_set)

    # Unmark (backtrack)
    self.visited_fragments.remove(fragment_name)
```

**Step 4:** Add Tests (1 hour)
```python
# tests/unit/core/test_fragment_cycles.py

def test_direct_fragment_cycle():
    """Fragment A references itself"""

def test_mutual_fragment_cycle():
    """Fragment A → B → A"""

def test_deep_fragment_cycle():
    """Fragment A → B → C → A"""

def test_fragment_type_mismatch():
    """Fragment on User applied to Post field"""

def test_valid_fragment_no_cycle():
    """Valid fragment with no cycles"""
```

**Dependencies:** Fragment resolver exists

**Risk Level:** Low (defensive programming)

---

## Part 3: Implementation Roadmap

### Phase 1: Foundation (Week 1)

**Priority:** Must-have
**Effort:** 8-10 hours
**Features:**
- [ ] Gap #1: Nested Field Fragments (2-3h)
- [ ] Gap #5: Fragment Cycle Detection (3-4h)
- [ ] Testing & Documentation (2-3h)

**Deliverable:** Complete fragment support

---

### Phase 2: Business Logic (Week 2)

**Priority:** Should-have
**Effort:** 6-8 hours
**Features:**
- [ ] Gap #2: Custom Directives (2-4h)
  - @rate_limit
  - @access_level
  - @cache
  - @validate
- [ ] Testing (2-3h)

**Deliverable:** Enterprise directive framework

---

### Phase 3: Performance (Week 3)

**Priority:** Should-have
**Effort:** 4-6 hours
**Features:**
- [ ] Gap #3: Auto DataLoader Integration (4-6h)
  - Registry
  - Auto-discovery
  - Per-request context
  - Cleanup

**Deliverable:** Automatic N+1 prevention

---

### Phase 4: Advanced (Week 4)

**Priority:** Nice-to-have
**Effort:** 6-8 hours
**Features:**
- [ ] Gap #4: HTTP Streaming / @stream support (6-8h)
  - Incremental delivery protocol
  - SSE response handling
  - @stream/@defer directives

**Deliverable:** Advanced streaming capabilities

---

## Part 4: Risk Analysis & Mitigation

### Risk 1: Breaking Existing Tests

**Probability:** Low
**Impact:** Medium
**Mitigation:**
- Run full test suite after each change (6000+ tests)
- Add feature flags for new capabilities
- Gradual rollout in test environment

---

### Risk 2: Performance Regression

**Probability:** Low
**Impact:** High
**Mitigation:**
- Benchmark tests for each feature
- Monitor query execution time
- Profile memory usage
- Use existing Rust pipeline for optimization

---

### Risk 3: Fragment Complexity Issues

**Probability:** Medium
**Impact:** Medium
**Mitigation:**
- Comprehensive cycle detection tests
- Type validation at parse time
- Depth limits for fragment expansion
- Complexity analyzer integration

---

### Risk 4: Directive Evaluation Performance

**Probability:** Medium
**Impact:** Medium
**Mitigation:**
- Lazy directive evaluation
- Caching of directive results
- Performance benchmarks
- Early exit for @skip directives

---

## Part 5: Success Criteria

### Phase 1 Success
- [ ] All nested fragment tests pass (5+)
- [ ] Fragment cycles detected and rejected (10+ edge cases)
- [ ] No regressions in existing fragment tests
- [ ] Performance unchanged (<5% variance)

### Phase 2 Success
- [ ] All directive tests pass (8+)
- [ ] Rate limiting enforced
- [ ] Access control working
- [ ] Caching applied correctly
- [ ] Documentation complete

### Phase 3 Success
- [ ] Auto-discovery working for all loaders
- [ ] Per-request isolation verified
- [ ] N+1 queries eliminated (benchmarked)
- [ ] Cleanup reliable across 100+ requests

### Phase 4 Success
- [ ] @stream directives working
- [ ] @defer directives working
- [ ] SSE streaming validated
- [ ] Incremental protocol spec-compliant

---

## Part 6: Testing Strategy

### Unit Tests
- Fragment cycle detection (10 tests)
- Type validation (8 tests)
- Directive evaluation (12 tests)
- DataLoader auto-discovery (8 tests)
- Streaming protocol (10 tests)

**Total Unit Tests:** ~50

### Integration Tests
- End-to-end nested fragments (5 tests)
- Multi-directive queries (5 tests)
- DataLoader performance (5 tests)
- Streaming with errors (5 tests)

**Total Integration Tests:** ~20

### Performance Tests
- Fragment resolution time
- DataLoader batching efficiency
- Streaming memory usage
- Directive evaluation overhead

**Total Performance Tests:** ~10

---

## Part 7: Documentation Requirements

### Developer Documentation
- [ ] Fragment support guide (nested, cycles, types)
- [ ] Custom directive framework guide
- [ ] DataLoader auto-integration guide
- [ ] HTTP streaming setup guide

### API Documentation
- [ ] @stream directive specification
- [ ] @defer directive specification
- [ ] Custom directive creation guide
- [ ] Loader registration guide

### Examples
- [ ] Nested fragment query examples
- [ ] Custom directive usage examples
- [ ] DataLoader integration example
- [ ] Streaming response handling

---

## Part 8: Dependencies & Prerequisites

### Existing Infrastructure Available
- ✅ Fragment resolver (exists and works)
- ✅ DataLoader implementation (exists and works)
- ✅ Subscription async framework (exists)
- ✅ RBAC directive patterns (exists)
- ✅ GraphQL-core integration (exists)

### Required Additions
- ⚠️ Registry pattern (can reuse existing patterns)
- ⚠️ Middleware integration (partially exists)
- ⚠️ Streaming response format (new)

### External Dependencies
- None (all GraphQL spec)

---

## Conclusion

FraiseQL has a solid foundation with 85-90% GraphQL spec compliance. The five easy-to-implement gaps represent excellent opportunities for incremental improvement:

1. **Nested Fragments** - Quick win, high value
2. **Directives** - Enterprise value
3. **Auto DataLoader** - Performance boost
4. **Fragment Cycles** - Stability improvement
5. **HTTP Streaming** - Advanced capability

**Total Effort to Complete All Gaps:** 18-28 hours
**Estimated Timeline:** 3-4 weeks at moderate pace

Each feature can be implemented independently, allowing prioritization based on business needs.

---

## Document Metadata

**Created:** 2025-12-17 11:15:00Z
**Version:** 1.0
**Status:** Complete - Ready for Implementation
**Reviewers:** Pending
**Next Steps:** Create detailed implementation tickets for each gap
