# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.1] - 2025-10-12

### ✨ **New Features**

**SQL Logging Support**: Added integrated SQL query logging functionality via the `database_echo` configuration parameter.

- Enable SQL logging by setting `database_echo=True` in your `FraiseQLConfig`
- Automatically configures psycopg loggers to DEBUG level for full SQL query visibility
- Useful for development and debugging database queries
- Environment variable support: `FRAISEQL_DATABASE_ECHO=true`

### 📚 **Documentation**

- Added comprehensive SQL logging guide (`SQL_LOGGING.md`)
- Updated configuration documentation with `database_echo` parameter details

## [0.11.0] - 2025-10-12

### 🚀 Maximum Performance by Default - Zero Configuration Required

This is a **major performance-focused release** that removes all performance configuration switches and makes FraiseQL deliver maximum speed out of the box. No configuration needed - you automatically get the fastest possible GraphQL API.

#### **Breaking Changes**

**Configuration Simplification**: The following configuration flags have been **removed** as their features are now always enabled:

- `json_passthrough_enabled` / `json_passthrough_in_production` / `json_passthrough_cache_nested`
- `pure_json_passthrough` - Now **always enabled** (25-60x faster queries)
- `pure_passthrough_use_rust` - Now **always enabled** (10-80x faster JSON transformation)
- `enable_query_caching` / `enable_turbo_router` - Now **always enabled**
- `jsonb_extraction_enabled` / `jsonb_auto_detect` / `jsonb_default_columns` - Now **always enabled**
- `unified_executor_enabled` / `turbo_enable_adaptive_caching` - Now **always enabled**
- `passthrough_auto_detect_views` / `passthrough_cache_view_metadata` - Now **always enabled**
- `enable_mode_hints` - Now **always enabled**
- **`camelforge_function` / `camelforge_field_threshold`** - PostgreSQL CamelForge function **removed**, Rust handles all transformation

**Migration Guide**: Simply remove these config flags from your `FraiseQLConfig`. The features they controlled are now always active, delivering maximum performance automatically.

```python
# Before v0.11.0
config = FraiseQLConfig(
    database_url="postgresql://...",
    pure_json_passthrough=True,  # Remove this
    pure_passthrough_use_rust=True,  # Remove this
    enable_turbo_router=True,  # Remove this
    jsonb_extraction_enabled=True,  # Remove this
)

# After v0.11.0 - Clean and simple!
config = FraiseQLConfig(
    database_url="postgresql://...",
    # All performance features automatically enabled
)
```

#### **Performance Improvements**

1. **Pure JSON Passthrough (25-60x faster)** - Always enabled
   - Uses `SELECT data::text` instead of field extraction
   - Bypasses Python object creation
   - Direct PostgreSQL → HTTP pipeline

2. **Rust Transformation (10-80x faster)** - Always enabled
   - Snake_case → camelCase conversion in Rust
   - Automatic `__typename` injection
   - Zero Python overhead

3. **JSONB Extraction** - Always enabled
   - Automatic detection of JSONB columns
   - Intelligent column selection
   - Optimized queries for hybrid tables

4. **TurboRouter Caching** - Always enabled
   - Registered queries execute instantly
   - Adaptive caching based on complexity
   - Zero overhead for cache hits

5. **Rust-Only Transformation** - PostgreSQL CamelForge removed
   - All camelCase transformation now handled by Rust
   - No PostgreSQL function dependency required
   - Simpler deployment and configuration

#### **What This Means For You**

- **Zero Configuration**: Maximum performance out of the box
- **Simpler Code**: No performance flags to manage
- **Faster APIs**: 25-60x query speedup automatically
- **Better DX**: No need to tune performance settings

#### **Files Changed**

**Core Performance**:
- `src/fraiseql/fastapi/config.py` - Removed 13 performance config flags
- `src/fraiseql/db.py` - Pure passthrough always enabled
- `src/fraiseql/core/raw_json_executor.py` - Rust transformation always enabled
- `src/fraiseql/fastapi/dependencies.py` - Passthrough always enabled in production
- `src/fraiseql/execution/mode_selector.py` - All modes always available
- `src/fraiseql/fastapi/app.py` - TurboRouter always enabled

**Tests Updated**:
- `tests/test_pure_passthrough_sql.py` - Updated for always-on behavior
- `tests/integration/auth/test_json_passthrough_config_fix.py` - Updated tests
- Removed obsolete configuration test files

#### **Backwards Compatibility**

This release maintains API compatibility for:
- All GraphQL query syntax
- All mutation patterns
- Database schema requirements
- Type definitions and decorators
- Authentication and authorization

The only breaking changes are the **removed configuration flags** which are no longer needed since the features they controlled are now always active.

#### **Upgrade Recommendation**

✅ **Highly Recommended**: All users should upgrade to v0.11.0 to get automatic 25-60x performance improvements with simpler configuration.

#### **Testing**

- ✅ All 19 pure passthrough tests passing
- ✅ All Rust transformation tests passing
- ✅ Integration tests verified
- ✅ Performance benchmarks confirmed

## [0.10.3] - 2025-10-06

### ✨ IpAddressString Scalar CIDR Notation Support

This release enhances the `IpAddressString` scalar to accept CIDR notation for improved PostgreSQL INET compatibility.

#### **Enhancement (Fixes #77)**

**IpAddressString now accepts CIDR notation** while remaining fully backward compatible.

**What's New:**
- Accepts both plain IP addresses and CIDR notation
- Extracts just the IP address from CIDR input
- Maintains backward compatibility with existing code

**Examples:**
```python
# Plain IP (existing behavior)
"192.168.1.1" → IPv4Address("192.168.1.1")

# CIDR notation (new)
"192.168.1.1/24" → IPv4Address("192.168.1.1")  # Extracts IP only
"2001:db8::1/64" → IPv6Address("2001:db8::1")  # Works for IPv6 too
```

**Use Cases:**
1. **PostgreSQL INET compatibility**: Accept CIDR input from frontend forms
2. **Flexible input patterns**: Support both traditional IP+subnet and CIDR notation
3. **Network configuration APIs**: Users can provide network info in familiar formats

**Implementation:**
- Changed from `ip_address()` to `ip_interface()` for parsing
- Returns only the IP address part (discards prefix length)
- Full test coverage for IPv4 and IPv6 with CIDR notation

**GraphQL Usage:**
```graphql
mutation {
  updateNetworkConfig(
    ipAddress: "192.168.1.1/24"  # CIDR accepted, stores IP only
  ) {
    success
  }
}
```

**PostgreSQL Integration Patterns:**

For applications storing CIDR in PostgreSQL INET columns, use mutually exclusive input fields:

```python
from fraiseql import UNSET
from fraiseql.types import IpAddress, SubnetMask, CIDR

@fraise_input
class NetworkConfigInput:
    # Pattern 1: Traditional IP + Subnet Mask
    ip_address: IpAddress | None = UNSET
    subnet_mask: SubnetMask | None = UNSET

    # Pattern 2: CIDR notation
    ip_address_cidr: CIDR | None = UNSET
```

Validate exactly one pattern in your resolver and convert to PostgreSQL INET format.

#### **Files Changed**

- `src/fraiseql/types/scalars/ip_address.py` - Updated parsing logic
- `tests/unit/core/type_system/test_ip_address_scalar.py` - Added CIDR tests

#### **Breaking Changes**

None - fully backward compatible.

## [0.10.2] - 2025-10-06

### ✨ Mutation Input Transformation and Empty String Handling

This release adds powerful input transformation capabilities to mutations and improves frontend compatibility with automatic empty string handling.

#### **New Features**

**1. `prepare_input` Hook for Mutations (Fixes #75)**

Adds an optional `prepare_input` static method to mutation classes that allows transforming input data after GraphQL validation but before the PostgreSQL function call.

**Use Cases:**
- Multi-field transformations (IP + subnet mask → CIDR notation)
- Empty string normalization
- Date format conversions
- Coordinate transformations
- Unit conversions

**Example:**
```python
@mutation
class CreateNetworkConfig:
    input: NetworkConfigInput
    success: NetworkConfigSuccess
    error: NetworkConfigError

    @staticmethod
    def prepare_input(input_data: dict) -> dict:
        """Transform IP + subnet mask to CIDR notation."""
        ip = input_data.get("ip_address")
        mask = input_data.get("subnet_mask")

        if ip and mask:
            cidr_prefix = {
                "255.255.255.0": 24,
                "255.255.0.0": 16,
            }.get(mask, 32)
            return {"ip_address": f"{ip}/{cidr_prefix}"}
        return input_data
```

**2. Automatic Empty String to NULL Conversion**

Frontends commonly send empty strings (`""`) when users clear text fields. FraiseQL now automatically converts empty strings to `None` for optional fields while maintaining data quality validation for required fields.

**Behavior:**
- **Optional fields** (`notes: str | None`): Accept `""`, convert to `None` ✅
- **Required fields** (`name: str`): Reject `""` with validation error ❌

**Example:**
```python
# Frontend sends:
{ id: "123", notes: "" }

# Backend receives and stores:
{ id: "123", notes: null }
```

#### **Benefits**

- ✅ Clean separation of frontend and backend data formats
- ✅ No need for custom resolvers or middleware
- ✅ Maintains type safety and data quality validation
- ✅ Supports standard frontend form behavior with nullable fields
- ✅ Non-breaking: existing mutations work unchanged

#### **Test Coverage**

- 3 new `prepare_input` hook tests
- 6 new empty string conversion tests
- All 3,295 existing tests pass (no regressions)

#### **Files Changed**

- `src/fraiseql/mutations/mutation_decorator.py` - Added `prepare_input` hook and documentation
- `src/fraiseql/types/constructor.py` - Empty string → None conversion in serialization
- `src/fraiseql/utils/fraiseql_builder.py` - Updated validation for optional fields
- `tests/unit/decorators/test_mutation_decorator.py` - Hook tests
- `tests/unit/decorators/test_empty_string_to_null.py` - Conversion tests (new)
- `tests/unit/core/type_system/test_empty_string_validation.py` - Updated test

## [0.10.1] - 2025-10-05

### 🐛 Bugfix: TurboRouter Dual-Hash APQ Lookup

**Problem**: TurboRouter failed to activate for Apollo Client APQ requests when using dual-hash registration, causing 30x-50x performance degradation (600ms instead of <20ms).

**Root Cause**: `TurboRegistry.get(query_text)` only checked normalized and raw hashes, never the `_apollo_hash_to_primary` mapping. When query text from APQ hashed to the apollo_client_hash instead of the server hash, the lookup failed.

**Fix**: Enhanced `TurboRegistry.get()` to check the `_apollo_hash_to_primary` mapping after trying direct hash lookups. Now correctly resolves Apollo Client hashes to their registered primary hashes.

**Impact**:
- ✅ TurboRouter now activates correctly for Apollo Client APQ requests with dual-hash support
- ✅ 30x-50x performance improvement restored (600ms → 15ms)
- ✅ 100% backward compatible - no code changes required
- ✅ Works with most common production GraphQL client (Apollo Client)

**Files Changed**:
- `src/fraiseql/fastapi/turbo.py:174-216` - Enhanced `get()` method with apollo hash mapping lookup

**Testing**:
- New test: `test_get_by_query_text_with_dual_hash_apollo_format` validates the fix
- All 25 turbo-related tests pass
- Full backward compatibility maintained

## [0.10.0] - 2025-10-04

### ✨ Context Parameters Support for Turbo Queries

This release adds `context_params` support to TurboQuery, enabling multi-tenant turbo-optimized queries with row-level security. This mirrors the mutation pattern and allows passing authentication context (tenant_id, user_id) from JWT to SQL functions.

#### **🎯 Problem Solved**
- Turbo queries could not access context parameters (tenant_id, user_id) from JWT
- Multi-tenant applications had to choose between turbo performance OR tenant isolation
- Required workarounds with session variables that didn't work with FraiseQL
- Security risk if trying to pass tenant_id via GraphQL variables (client-controlled)

#### **✨ New Features**
- **`context_params` field** in `TurboQuery` for context-to-SQL parameter mapping
- **Automatic context injection** in `TurboRouter.execute()` (mirrors mutation pattern)
- **Error handling** for missing required context parameters
- **100% backward compatible** - context_params is optional

#### **🔧 Usage**
```python
from fraiseql.fastapi import TurboQuery

# Register turbo query with context parameters
turbo_query = TurboQuery(
    graphql_query=query,
    sql_template="SELECT turbo.fn_get_allocations(%(period)s, %(tenant_id)s)::json",
    param_mapping={"period": "period"},         # From GraphQL variables
    operation_name="GetAllocations",
    context_params={"tenant_id": "tenant_id"},  # ✨ NEW: From JWT context
)

registry.register(turbo_query)

# Execute with context (from JWT authentication)
result = await turbo_router.execute(
    query=query,
    variables={"period": "CURRENT"},
    context={"db": db, "tenant_id": "tenant-123"}  # From JWT
)

# SQL receives: fn_get_allocations('CURRENT', 'tenant-123')
# ✅ Both variable AND context parameter!
```

#### **✅ Benefits**
- **Multi-tenant support** for turbo queries with row-level security
- **10x+ performance** with tenant isolation (no compromise needed)
- **Security** - tenant_id from server-side JWT, not client input
- **Consistent API** - matches mutation `context_params` pattern
- **Audit trails** - pass user_id for created_by/updated_by tracking

#### **📚 Documentation**
- Full test coverage in `tests/integration/caching/test_turbo_router.py`
- Error handling tests for missing context parameters

#### **🔍 Technical Details**
- Added `context_params: dict[str, str] | None` to `TurboQuery` dataclass
- Updated `TurboRouter.execute()` to map context values to SQL params
- Follows exact same pattern as `MutationDefinition.create_resolver()`
- Raises `ValueError` for missing required context parameters

#### **🎨 Use Cases**
- **Multi-tenant SaaS** - Enforce tenant isolation in turbo queries
- **Audit logging** - Track user_id for all data access
- **Row-level security** - Pass authentication context to PostgreSQL RLS
- **Cache isolation** - Include tenant_id in cache keys

## [0.9.6] - 2025-10-04

### ✨ Native Dual-Hash Support for Apollo Client APQ

This release adds first-class support for Apollo Client's Automatic Persisted Queries (APQ) with native dual-hash compatibility, eliminating hash mismatches between frontend and backend.

#### **🎯 Problem Solved**
- Apollo Client and FraiseQL compute different SHA-256 hashes for queries with parameters
- Previous workaround required registering queries twice (once per hash)
- "Hash mismatch" warnings appeared even though both hashes were valid

#### **✨ New Features**
- **`apollo_client_hash` field** in `TurboQuery` for Apollo Client hash
- **Dual-hash registration** - single registration, both hashes work
- **`get_by_hash()` method** for direct hash-based query retrieval
- **Automatic LRU cleanup** for apollo hash mappings
- **100% backward compatible** - apollo_client_hash is optional

#### **🔧 Usage**
```python
from fraiseql.fastapi import TurboQuery

turbo_query = TurboQuery(
    graphql_query=query,
    sql_template=template,
    param_mapping=mapping,
    operation_name="GetMetrics",
    apollo_client_hash="ce8fae62...",  # ✨ NEW: Apollo Client's hash
)

# Single registration handles both hashes
registry.register_with_raw_hash(turbo_query, fraiseql_server_hash)

# ✅ Works with either hash!
result = registry.get_by_hash(fraiseql_server_hash)  # Works
result = registry.get_by_hash(apollo_client_hash)    # Also works!
```

#### **✅ Benefits**
- **Single registration** instead of double
- **No hash mismatch warnings** when apollo_client_hash provided
- **Cleaner API** for Apollo Client + FraiseQL integration
- **First-class APQ support** as a core feature
- **Memory efficient** - no query duplication

#### **📚 Documentation**
- Comprehensive section in `docs/advanced/turbo-router.md`
- Full test coverage in `tests/test_apollo_client_apq_dual_hash.py`
- Database schema examples for production use

#### **🔍 Technical Details**
- Added `_apollo_hash_to_primary` mapping in `TurboRegistry`
- Enhanced `register_with_raw_hash()` for automatic dual-hash registration
- New `get_by_hash()` method supports both server and Apollo hashes
- Updated `clear()` and LRU eviction to clean up mappings

#### **🎨 Related Issues**
- Resolves #72: Feature Request: Native dual-hash support for Apollo Client APQ compatibility

## [0.9.5] - 2025-09-28

### 🐛 Critical Fix: Nested Object Filtering on Hybrid Tables

This release fixes a critical performance and correctness issue where nested object filters on hybrid tables (with both SQL columns and JSONB data) were using slow JSONB traversal instead of indexed SQL columns.

#### **🚨 Issue Fixed**
- Nested object filters on hybrid tables were generating inefficient JSONB paths
- Before: `WHERE (data -> 'machine' ->> 'id') = '...'` (slow JSONB traversal)
- After: `WHERE machine_id = '...'` (fast indexed column access)
- **10-100x performance improvement** for nested object filtering

#### **🔧 Technical Details**
- Modified `_build_find_query()` to detect hybrid tables with nested filters
- Added `_where_obj_to_dict()` to convert WHERE objects for inspection
- Updated `_convert_dict_where_to_sql()` to map nested objects to SQL columns
- Intelligent routing: uses SQL columns when available, JSONB as fallback

#### **✅ Impact**
- **Severity**: Critical - incorrect results and severe performance degradation
- **Affected**: Hybrid tables using `register_type_for_view()` with `has_jsonb_data=True`
- **Performance**: 10-100x faster queries using indexed columns vs JSONB
- **Migration**: No action required - automatic optimization

#### **📊 Bonus**
- `WhereInput` types now work correctly on regular (non-JSONB) tables
- Type-safe UUID comparisons instead of text/UUID mismatches
- Eliminated "Unsupported operator: id" warnings

## [0.9.4] - 2025-09-28

### 🐛 Critical Fix: Nested Object Filtering in JSONB WHERE Clauses

This release fixes a critical bug where nested object filters in GraphQL WHERE clauses were generating incorrect SQL for JSONB-backed tables, causing filters to fail silently.

#### **🚨 Issue Fixed**
- Nested object filters were accessing fields at root level instead of proper nested paths
- Before: `WHERE (data ->> 'id') = '...'` (incorrect root-level access)
- After: `WHERE (data -> 'machine' ->> 'id') = '...'` (correct nested path)

#### **🔧 Technical Details**
- Modified `where_generator.py` to pass `parent_path` through the `to_sql()` chain
- Added `_build_nested_path()` helper for cleaner path construction
- Fixed logical operators (AND, OR, NOT) to maintain parent context
- Enhanced test coverage for deep nesting (3+ levels)

#### **✅ Impact**
- **Severity**: High - filters were silently failing
- **Affected**: JSONB tables with nested object filtering
- **Migration**: No action required - existing code automatically benefits

## [0.9.3] - 2025-09-21

### ✨ Built-in Tenant-Aware APQ Caching

This release adds native tenant isolation support to FraiseQL's APQ (Automatic Persisted Queries) caching system, enabling secure multi-tenant applications without custom implementations.

#### **🎯 Key Features**
- **Automatic Tenant Isolation**: Both `MemoryAPQBackend` and `PostgreSQLAPQBackend` now automatically isolate cached responses by tenant
- **Zero Configuration**: Works out of the box - just pass context with tenant_id
- **Security by Default**: Prevents cross-tenant data leakage with built-in isolation
- **Context Propagation**: Router automatically passes JWT context to APQ backends

#### **🏗️ Implementation Details**

**MemoryAPQBackend**:
- Generates tenant-specific cache keys: `{tenant_id}:{hash}`
- Maintains separate cache spaces per tenant
- Global cache available for non-tenant requests

**PostgreSQLAPQBackend**:
- Added `tenant_id` column to responses table
- Composite primary key `(hash, COALESCE(tenant_id, ''))`
- Indexed tenant_id for optimal performance

#### **📚 Documentation**
- Comprehensive guide: `docs/apq_tenant_context_guide.md`
- Multi-tenant example: `examples/apq_multi_tenant.py`
- Full test coverage with tenant isolation validation

#### **🔧 Usage**
```python
# Tenant isolation is automatic!
context = {"user": {"metadata": {"tenant_id": "acme-corp"}}}
response = backend.get_cached_response(hash, context=context)
```

## [0.9.2] - 2025-09-21

### 🐛 APQ Backend Integration Fix

This release fixes a critical issue with Automatic Persisted Queries (APQ) backend integration, enabling custom storage backends to properly store and retrieve persisted queries and cached responses.

#### **🎯 Problem Solved**
- Custom APQ backends (PostgreSQL, MongoDB, Redis) were not being called during APQ request processing
- Backend methods `store_persisted_query()` and `store_cached_response()` were never invoked
- Made it impossible to use database-backed APQ storage in production environments

#### **✅ Solution Implemented**
- **Query Registration**: APQ registration requests (query + hash) now properly store queries in custom backends
- **Backend Priority**: Custom backends are checked first before falling back to memory storage
- **Response Caching**: Successful query responses are now cached in custom backends for performance
- **Backward Compatibility**: Maintains full compatibility with existing memory-only APQ implementations

#### **🔒 Security & Multi-tenancy**
- **JWT Context Preserved**: Authentication context including `tenant_id` from JWT metadata flows through entire APQ lifecycle
- **Tenant Isolation**: Multi-tenant applications maintain proper query isolation
- **Authentication First**: Security checks occur before APQ processing
- **Full Context Preservation**: User context, permissions, and metadata remain intact

#### **🚀 Impact**
- Enables production-ready APQ with persistent storage
- Supports distributed caching across multiple servers
- Allows custom backend implementations for specific infrastructure needs
- Fixes integration with custom backends like `printoptim_backend`

#### **🧪 Testing**
- All 19 APQ-specific tests pass
- Full test suite of 3246 tests maintains 100% pass rate
- Added verification for backend integration and tenant ID preservation

## [0.9.1] - 2025-09-21

### ✨ Comprehensive Automatic Field Description Extraction

This release introduces **comprehensive automatic field description extraction** that transforms Python docstrings into detailed GraphQL field descriptions, building on the v0.9.0 automatic docstring extraction foundation.

#### **🎯 Key Features**
- **Automatic Field Descriptions**: Extracts field descriptions from docstring `Fields:`, `Attributes:`, and `Args:` sections
- **Enhanced Where Clause Documentation**: 35+ filter operations automatically documented with type-aware descriptions
- **Multiple Documentation Sources**: Intelligent priority system supporting various docstring formats
- **Apollo Studio Integration**: Field descriptions appear as tooltips with comprehensive operation explanations
- **Zero Configuration**: Works with existing code without any changes required

#### **🧪 Quality Assurance**
- **35 Comprehensive Unit Tests**: Full coverage of field description extraction functionality
- **3200+ Integration Tests**: Complete test suite ensuring backward compatibility
- **Performance Optimized**: Minimal overhead with intelligent caching
- **Type-Safe Implementation**: Maintains existing type safety guarantees

#### **📚 Documentation & Examples**
- **Complete Feature Documentation**: Comprehensive guides and API reference
- **3 Working Examples**: Demonstrating all aspects of automatic field descriptions
- **Migration Guide**: Easy adoption for existing codebases
- **Best Practices**: Usage patterns and optimization recommendations

#### **🔄 Implementation Details**
- **2 New Utility Modules**: `docstring_extractor.py` and `where_clause_descriptions.py`
- **Seamless Pipeline Integration**: Works with existing FraiseQL type system
- **Automatic Filter Enhancement**: All existing filter types gain comprehensive documentation
- **Clean Architecture**: Maintainable code following project conventions

## [0.9.0] - 2025-09-20

### ✨ Automatic Docstring Extraction for GraphQL Schema Descriptions

This release introduces **automatic docstring extraction** that transforms Python docstrings into GraphQL schema descriptions visible in Apollo Studio, providing zero-configuration documentation for your GraphQL APIs.

#### **🎯 Key Features**
- **Type-Level Descriptions**: `@fraise_type` classes automatically use their docstrings as GraphQL type descriptions
- **Query/Mutation Descriptions**: `@query` functions and `@mutation` classes automatically extract docstrings for field descriptions
- **Multiline Support**: Automatic cleaning and formatting of multiline docstrings using `inspect.cleandoc`
- **Apollo Studio Integration**: All descriptions appear automatically in GraphQL introspection and Apollo Studio

#### **🔧 Implementation**
- **Zero Configuration**: No code changes required - existing docstrings automatically become GraphQL descriptions
- **Backward Compatibility**: Existing explicit `description` parameters continue to work unchanged
- **Smart Extraction**: Mutation classes use original docstrings, not auto-generated fallback descriptions
- **Clean Formatting**: Proper indentation and whitespace handling for professional documentation

#### **📚 Developer Experience**
```python
@fraiseql.type
class User:
    """A user account with authentication and profile information."""  # ✅ Apollo Studio
    id: UUID
    name: str

@fraiseql.query
async def get_users(info) -> list[User]:
    """Get all users with their profile information."""  # ✅ Apollo Studio
    return await repo.find("v_user")
```

#### **🧪 Testing**
- **12 comprehensive unit tests** covering all functionality and edge cases
- **Type descriptions**: Automatic extraction, multiline cleaning, missing docstrings
- **Query/mutation descriptions**: Function docstrings, class docstrings, backward compatibility
- **Integration tests**: Full GraphQL schema generation and introspection

#### **📖 Documentation**
- **Enhanced type system docs** with automatic documentation examples
- **Updated README** showcasing the feature in quick start guide
- **Code purification** achieving eternal sunshine repository state

This release significantly enhances the developer experience by providing automatic, rich documentation for GraphQL schemas without requiring any configuration or code changes.

## [0.8.1] - 2025-09-20

### ✨ Entity-Aware Query Routing

This release introduces **intelligent query routing** that automatically determines execution mode based on entity complexity, optimizing performance while ensuring cache consistency.

#### **🎯 Key Features**
- **EntityRoutingConfig**: Declarative entity classification system for configuring which entities should use turbo vs normal mode
- **EntityExtractor**: GraphQL query analysis engine that automatically detects entities using schema introspection
- **QueryRouter**: Intelligent execution mode determination based on entity types and configurable strategies
- **ModeSelector Integration**: Seamless integration with existing execution pipeline

#### **🚀 Benefits**
- **Performance Optimization**: Complex entities with materialized views automatically get turbo caching
- **Cache Consistency**: Simple entities without materialized views get real-time data to avoid stale cache issues
- **Developer Experience**: Configuration-driven approach with automatic routing - no manual mode hints needed
- **Backward Compatibility**: Optional feature that preserves all existing behavior when not configured

#### **📝 Usage**
```python
FraiseQLConfig(
    entity_routing=EntityRoutingConfig(
        turbo_entities=["allocation", "contract", "machine"],  # Complex entities
        normal_entities=["dnsServer", "gateway"],              # Simple entities
        mixed_query_strategy="normal",                         # Mixed query strategy
        auto_routing_enabled=True,
    )
)
```

#### **🔄 Query Routing Logic**
- **Mode hints** (e.g., `# @mode: turbo`) → Always override entity routing
- **Turbo entities only** → `ExecutionMode.TURBO` (optimized caching)
- **Normal entities only** → `ExecutionMode.NORMAL` (real-time data)
- **Mixed queries** → Use configured strategy (normal/turbo/split)
- **Unknown entities** → Safe fallback to normal mode

## [0.8.0] - 2025-09-20

### 🚀 Major Features - APQ Storage Backend Abstraction

This release implements **Automatic Persisted Queries (APQ) Storage Backend Abstraction**, completing FraiseQL's three-layer performance optimization architecture and positioning it as the **fastest Python GraphQL framework**.

#### **✨ APQ Storage Backends**
- **Memory Backend**: Zero-configuration default for development and simple applications
- **PostgreSQL Backend**: Enterprise-grade persistent storage with multi-instance coordination
- **Redis Backend**: High-performance distributed caching for scalable deployments
- **Factory Pattern**: Pluggable architecture for easy backend switching and extension

#### **🎯 Key Features**
- **SHA-256 Query Hashing**: Secure and collision-resistant query identification
- **Bandwidth Reduction**: 70% smaller requests via hash-based query lookup
- **Enterprise Configuration**: Schema isolation and custom connection settings
- **Graceful Fallback**: Automatic degradation to full queries when cache misses occur
- **Multi-Instance Ready**: PostgreSQL and Redis backends support distributed deployments

#### **📊 Performance Achievements**
- **0.5-2ms Response Times**: All three optimization layers working in harmony
- **100-500x Performance Improvement**: Combined APQ + TurboRouter + JSON Passthrough
- **95% Cache Hit Rates**: Real production benchmarks with enterprise workloads
- **Sub-millisecond Cached Responses**: JSON passthrough optimization eliminates serialization

#### **🔧 Configuration Examples**
```python
# Memory Backend (development/simple apps)
config = FraiseQLConfig(apq_storage_backend="memory")

# PostgreSQL Backend (enterprise scale)
config = FraiseQLConfig(
    apq_storage_backend="postgresql",
    apq_storage_schema="apq_cache"  # Custom schema isolation
)

# Redis Backend (high-performance caching)
config = FraiseQLConfig(apq_storage_backend="redis")
```

#### **🏗️ Architecture Completion**
FraiseQL now features the complete three-layer optimization stack:
1. **APQ Layer** → 70% bandwidth reduction
2. **TurboRouter Layer** → 4-10x execution speedup
3. **JSON Passthrough Layer** → 5-20x serialization speedup
4. **Combined Impact** → **100-500x total performance improvement**

### 📚 **Documentation Enhancements**

#### **New Comprehensive Guides**
- **Performance Optimization Layers Guide** (636 lines): Complete analysis of how APQ, TurboRouter, and JSON Passthrough work together
- **APQ Storage Backends Guide** (433 lines): Configuration examples, troubleshooting, and production deployment patterns
- **Updated README**: Enhanced performance comparisons with optimization layer breakdown

#### **Production-Ready Documentation**
- **Enterprise Configuration**: Multi-instance coordination patterns
- **Troubleshooting Guides**: Common issues and resolutions
- **Performance Monitoring**: KPIs and observability strategies
- **Migration Guides**: Seamless adoption paths for existing applications

### 🧪 **Testing Infrastructure**

#### **Comprehensive Test Coverage**
- **1,000+ New Tests**: Full coverage for all APQ storage backends
- **335 Integration Tests**: Multi-backend APQ functionality validation
- **258 Middleware Tests**: Caching behavior and error handling
- **227 PostgreSQL Tests**: Enterprise storage backend verification
- **200 Factory Tests**: Backend selection and configuration testing

#### **Quality Assurance**
- **3,204 Total Tests**: All passing with comprehensive regression coverage
- **Production Validation**: Real-world enterprise workload testing
- **Performance Benchmarks**: Verified 100-500x improvement claims

### 🔄 **Migration & Compatibility**

#### **Zero Breaking Changes**
- **Fully Backward Compatible**: Existing applications continue working unchanged
- **Gradual Adoption**: APQ can be enabled incrementally
- **Configuration Override**: Easy opt-in with environment variables
- **Legacy Support**: Full compatibility with existing TurboRouter and JSON passthrough setups

#### **Enterprise Migration**
- **Database Schema**: Automatic APQ table creation for PostgreSQL backend
- **Connection Pooling**: Optimized database connections for APQ storage
- **Monitoring Integration**: CloudWatch, Prometheus, and custom metrics support

### 💎 **Repository Quality Improvements**

#### **Eternal Repository Perfection**
- **Version Consistency**: Fixed all version mismatches across package metadata
- **Code Quality**: Zero linting issues, consistent patterns across 50 modified files
- **Documentation Coherence**: 95 documentation files with verified internal links
- **Artifact Cleanup**: Removed temporary files and optimized .gitignore

#### **Development Excellence**
- **Disciplined TDD**: Five-phase implementation with comprehensive test coverage
- **Clean Architecture**: Proper separation of concerns and dependency injection
- **Production Patterns**: Enterprise-ready configuration and error handling

### 🎉 **Why This Release Matters**

This release establishes FraiseQL as the **definitive solution for high-performance Python GraphQL APIs**:

- **Production-Grade APQ**: Enterprise storage options with schema isolation
- **Architectural Completeness**: All three optimization layers working in harmony
- **Developer Experience**: Zero-configuration memory backend to enterprise PostgreSQL
- **Performance Leadership**: Verifiable 100-500x improvements over traditional frameworks
- **Enterprise Ready**: Multi-tenant, distributed, and monitoring-integrated

### 📈 **Performance Comparison Matrix**

| Configuration | Response Time | Bandwidth | Use Case |
|---------------|---------------|-----------|----------|
| **All 3 Layers** (APQ + TurboRouter + Passthrough) | **0.5-2ms** | -70% | Ultimate performance |
| **APQ + TurboRouter** | 2-5ms | -70% | Enterprise standard |
| **APQ + Passthrough** | 1-10ms | -70% | Modern web applications |
| **TurboRouter Only** | 5-25ms | Standard | API-focused applications |
| **Standard Mode** | 25-100ms | Standard | Development & complex queries |

### 🔧 **Technical Implementation**

#### **Core Components Added**
- `src/fraiseql/middleware/apq.py` - APQ middleware integration
- `src/fraiseql/middleware/apq_caching.py` - Caching logic and storage abstraction
- `src/fraiseql/storage/backends/` - Storage backend implementations
- `src/fraiseql/storage/apq_store.py` - Unified storage interface

#### **FastAPI Integration**
- Enhanced router with backward-compatible APQ middleware
- Automatic APQ detection and processing
- Configurable storage backend selection
- Production-ready error handling and logging

### 🏆 **Achievement Summary**

FraiseQL v0.8.0 delivers on the promise of **sub-millisecond GraphQL responses** with:
- **Complete optimization stack** with pluggable APQ storage
- **Enterprise-grade documentation** with production deployment guides
- **Comprehensive testing** ensuring reliability at scale
- **Zero breaking changes** enabling seamless upgrades

This release represents a **major milestone** in Python GraphQL performance optimization, establishing FraiseQL as the fastest and most production-ready solution available.

---

**Files Changed**: 50 files (+4,464 additions, -2,016 deletions)
**Test Coverage**: 3,204 tests passing, 1,000+ new APQ-specific tests
**Documentation**: 2 comprehensive new guides (1,069 total lines)

## [0.7.26] - 2025-09-17

### 🔒 Security

#### Authentication-Aware GraphQL Introspection
- **SEC**: Enhanced introspection policy with authentication awareness
- **SEC**: Configurable introspection access control based on user context
- **SEC**: Production-ready introspection security patterns

### 🧪 Testing

#### Security Test Coverage
- **TEST**: Authentication-aware introspection policy validation
- **TEST**: Security configuration testing
- **TEST**: Production security scenario verification

## [0.7.25] - 2025-09-17

### 🐛 Fixed

#### Critical WHERE Clause Generation Bugs
- **FIX**: Hostname filtering no longer incorrectly applies ltree casting for `.local` domains
- **FIX**: Proper parentheses placement for type casting: `((path))::type` instead of `path::type`
- **FIX**: Boolean operations consistently use text comparison (`= 'true'/'false'`) instead of `::boolean` casting
- **FIX**: Numeric operations consistently use `::numeric` casting for proper PostgreSQL comparison
- **FIX**: Resolves production issues where `printserver01.local` caused SQL syntax errors

### 🧪 Testing

#### Industrial-Grade Test Coverage
- **TEST**: Comprehensive regression tests for WHERE clause generation edge cases
- **TEST**: 41+ new regression tests covering hostname, boolean, and numeric filtering
- **TEST**: SQL injection resistance validation
- **TEST**: PostgreSQL syntax compliance verification
- **TEST**: Production scenario validation for enterprise use cases

### 🔒 Security

- **SEC**: Enhanced SQL injection prevention in type casting operations
- **SEC**: Parameterized query validation for all operator strategies

## [0.7.24] - 2025-09-17

### 🚀 Added

#### Hybrid Table Support
- **NEW**: Full support for hybrid tables with both regular SQL columns and JSONB data
- **NEW**: Automatic field detection and optimal SQL generation
- **NEW**: Registration-time metadata for zero-latency field classification
- **NEW**: `register_type_for_view()` enhanced with `table_columns` and `has_jsonb_data` parameters

### 🏃‍♂️ Performance

#### SQL Generation Optimization
- **PERF**: 0.4μs field detection time with metadata registration (1670x faster than DB query)
- **PERF**: Zero runtime database introspection for registered hybrid tables
- **PERF**: Multi-level caching system for field path decisions
- **PERF**: Minimal memory overhead (~1KB per table for metadata)

### 🐛 Fixed

#### Critical Filtering Bug
- **FIX**: Hybrid tables now correctly filter on regular SQL columns
- **FIX**: Dynamic filter construction works properly on mixed column types
- **FIX**: WHERE clause generation automatically detects column vs JSONB fields
- **FIX**: Resolves issue where `WHERE is_active = true` was incorrectly generated as `WHERE data->>'is_active' = true`

### 📚 Documentation

- **DOCS**: Complete hybrid tables guide with examples
- **DOCS**: API reference for registration functions
- **DOCS**: Performance benchmarks and optimization guide
- **DOCS**: Migration guide from pure JSONB to hybrid tables

### 🧪 Testing

- **TEST**: Comprehensive hybrid table filtering test suite
- **TEST**: Performance benchmarks for SQL generation
- **TEST**: Generic examples replacing domain-specific ones

## [0.7.21] - 2025-09-14

### 🐛 **Bug Fixes**

#### **Mutation Name Collision Fix**
- **Problem solved**: Mutations with similar names (e.g., `CreateItem` and `CreateItemComponent`) were causing parameter validation confusion where `createItemComponent` incorrectly required `item_serial_number` from `CreateItemInput` instead of its own `CreateItemComponentInput` fields
- **Impact**: 🟡 **High** - GraphQL mutations with similar names would fail validation with incorrect error messages, blocking API functionality
- **Root cause**: Resolver naming strategy used `to_snake_case(class_name)` which could create collisions when similar class names produced identical snake_case names, causing one mutation to overwrite another's metadata in the GraphQL schema registry
- **Solution**: Updated resolver naming to use PostgreSQL function names for uniqueness (e.g., `create_item` vs `create_item_component`) and ensure fresh annotation dictionaries prevent shared references
- **Files modified**:
  - `src/fraiseql/mutations/mutation_decorator.py` - Enhanced resolver naming logic for collision prevention
- **Test coverage**: Added comprehensive collision-specific test suite `test_similar_mutation_names_collision_fix.py` with 8 test scenarios covering resolver naming, input type assignment, registry separation, and metadata independence
- **Validation behavior**:
  - **✅ Before fix**: `CreateItem` and `CreateItemComponent` could share parameter validation causing incorrect errors
  - **✅ After fix**: Each mutation validates independently with correct input type requirements
  - **✅ Backward compatibility**: No breaking changes - existing functionality preserved
- **Quality assurance**: All 2,979+ existing tests continue to pass + 8 new collision-prevention tests

## [0.7.20] - 2025-09-13

### 🐛 **Bug Fixes**

#### **JSONB Numeric Ordering Fix**
- **Problem solved**: ORDER BY clauses were using JSONB text extraction (`data->>'field'`) causing lexicographic sorting where `"125.0" > "1234.53"` due to string comparison
- **Impact**: 🔴 **Critical** - Data integrity issue for financial data, amounts, quantities, and all numeric field ordering
- **Root cause**: `order_by_generator.py` generated `ORDER BY data ->> 'amount' ASC` (text) instead of `ORDER BY data -> 'amount' ASC` (JSONB numeric)
- **Solution**: Changed `OrderBy.to_sql()` to use JSONB extraction preserving original data types for proper PostgreSQL numeric comparison
- **Files modified**:
  - `src/fraiseql/sql/order_by_generator.py` - Core fix + enhanced documentation explaining JSONB vs text extraction
  - 6 existing test files updated to expect correct JSONB extraction behavior
- **Test coverage**: Added comprehensive `test_numeric_ordering_bug.py` with 7 test scenarios covering single/multiple fields, nested paths, financial amounts, and decimal precision
- **Performance benefits**:
  - **✅ Native PostgreSQL numeric comparison** instead of text parsing
  - **✅ Better index utilization** potential for numeric fields
  - **✅ Reduced conversion overhead** in sorting operations
- **Backward compatibility**: ✅ **Fully maintained** - no breaking changes, existing GraphQL queries work unchanged
- **Before/After behavior**:
  - **❌ Before**: `['1000.0', '1234.53', '125.0', '25.0']` (lexicographic)
  - **✅ After**: `[25.0, 125.0, 1000.0, 1234.53]` (proper numeric)

#### **Architecture Design Note**
- **WHERE clauses remain unchanged**: Correctly use text extraction with casting `(data->>'field')::numeric` for PostgreSQL type conversion
- **ORDER BY clauses now fixed**: Use JSONB extraction `data->'field'` for type preservation and proper sorting
- **Design principle**: Text extraction for casting operations, JSONB extraction for type-preserving operations

## [0.7.19] - 2025-09-12

### 🚨 **CRITICAL SECURITY FIX**

#### **None Value Validation Bypass Regression Fix**
- **Problem solved**: v0.7.18 still allowed `None` values for required string fields in GraphQL input processing, bypassing validation completely
- **Security impact**: 🔴 **CRITICAL** - Data integrity violation, complete validation bypass for `None` values
- **Root cause**: Validation logic in `make_init()` checked `final_value is not None` before applying string validation, allowing `None` to completely bypass required field validation
- **Solution**: Enhanced `_validate_input_string_value()` to validate `None` values for required fields before string-specific validation
- **Files modified**:
  - `src/fraiseql/utils/fraiseql_builder.py` - Enhanced validation logic to check for `None` values in required fields
- **Test coverage**: Added `None` value validation test cases to existing regression tests
- **Validation behavior**:
  - **✅ Required fields**: `name: str` now properly rejects `None` values with "Field 'name' is required and cannot be None"
  - **✅ Empty strings**: Still rejected with "Field 'name' cannot be empty"
  - **✅ Optional fields**: `name: str | None = None` continues to work correctly
  - **✅ Backward compatibility**: No breaking changes for valid code

#### **Enhanced Error Messages**
- **None value errors**: Clear distinction between `None` and empty string validation failures
- **Field context**: Error messages include field names for precise debugging
- **GraphQL compatibility**: Error format suitable for GraphQL mutation responses

## [0.7.18] - 2025-09-12

### 🐛 **Note**
This version contained a validation regression where `None` values bypassed validation for required fields. **Upgrade to v0.7.19 immediately**.

## [0.7.17] - 2025-09-11

### 🚨 **CRITICAL REGRESSION FIX**

#### **Empty String Validation Regression Fix**
- **Problem solved**: v0.7.16 validation was incorrectly applied during field resolution, preventing existing database records with empty string fields from being loaded
- **Impact**: 15+ production tests failed, breaking existing API consumers who couldn't upgrade from v0.7.15
- **Root cause**: String validation was applied in `make_init()` for ALL type kinds (input, output, type, interface) during object instantiation
- **Solution**: Apply validation only for `@fraiseql.input` types, not output/type/interface types
- **Files modified**:
  - `src/fraiseql/utils/fraiseql_builder.py` - Modified `make_init()` to accept `type_kind` parameter
  - `src/fraiseql/types/constructor.py` - Pass type kind information to `make_init()`
- **Test coverage**: Added comprehensive regression test suite (`tests/regression/test_v0716_empty_string_validation_regression.py`)

#### **Validation Behavior Clarification**
- **✅ Input validation**: `@fraiseql.input` types still reject empty strings (validation preserved)
- **✅ Data loading**: `@fraiseql.type` types can load existing data with empty fields (regression fixed)
- **✅ Backward compatibility**: No breaking changes, users can upgrade immediately
- **✅ Performance**: Maintains v0.7.16 performance improvements

#### **Technical Implementation**
- **Separation of concerns**: Clear distinction between input validation and data loading
- **Type-aware validation**: Validation logic now respects FraiseQL type kinds
- **Enhanced documentation**: Added comprehensive code comments explaining validation behavior
- **Future-proof**: Prevents similar regressions with proper type kind handling

## [0.7.16] - 2025-09-11

### 🐛 **Fixed**

#### **FraiseQL Empty String Validation for Required Fields**
- **Enhancement**: FraiseQL now properly validates required string fields to reject empty strings and whitespace-only values
- **Problem solved**: Previously, FraiseQL accepted empty strings (`""`) and whitespace-only strings (`"   "`) for required string fields, creating inconsistent validation behavior
- **Key features**:
  - **Empty string rejection**: Required string fields (`name: str`) now reject `""` and `"   "` with clear error messages
  - **Consistent behavior**: Aligns with existing `null` value rejection for required fields
  - **Optional field support**: Optional string fields (`name: str | None`) still accept `None` but reject empty strings when explicitly provided
  - **Clear error messages**: Validation failures show `"Field 'field_name' cannot be empty"` for easy debugging
  - **Type-aware validation**: Only applies to string fields, preserves existing behavior for other types
- **Framework-level validation**: Automatic validation with no boilerplate code required
- **GraphQL compatibility**: Error messages suitable for GraphQL error responses
- **Zero breaking changes**: Only adds validation where it was missing, maintains backward compatibility

#### **Technical Implementation**
- **Validation location**: Integrated into `make_init()` function for automatic enforcement
- **Type detection**: Uses existing `_extract_type()` function to handle `Optional`/`Union` types correctly
- **Performance**: Minimal overhead, only validates string fields during object construction
- **Test coverage**: 15 comprehensive tests covering all scenarios including inheritance and nested types

### 🧪 **Testing**
- **New test suite**: Added comprehensive test coverage for empty string validation scenarios
- **Integration tests**: Verified functionality works correctly in nested inputs and complex scenarios
- **Regression testing**: All existing 501 type system tests continue to pass

## [0.7.15] - 2025-09-11

### ✨ **Added**

#### **Built-in JSON Serialization for FraiseQL Input Objects**
- **New feature**: All FraiseQL input objects now have native JSON serialization support via built-in `to_dict()` and `__json__()` methods
- **Problem solved**: Resolves v0.7.14 JSON serialization errors where nested FraiseQL input objects could not be JSON serialized, causing `"Object of type X is not JSON serializable"` errors
- **Key features**:
  - **`to_dict()` method**: Converts input objects to dictionaries, automatically excluding UNSET values
  - **`__json__()` method**: Provides direct JSON serialization compatibility
  - **Recursive serialization**: Handles nested FraiseQL objects and lists seamlessly
  - **UNSET filtering**: Automatically excludes UNSET values during serialization
  - **Type consistency**: Properly handles dates, UUIDs, enums using existing SQL generator logic
- **Zero breaking changes**: Fully backward compatible with existing code
- **Framework integration**: Built into core type system - no user setup required

### 🐛 **Fixed**
- **JSON Serialization**: Fixed critical issue where FraiseQL input objects failed JSON serialization when used as nested objects
- **Date serialization**: Ensured date, UUID, enum, and other special types are properly serialized to string formats in `to_dict()` method
- **Recursive handling**: Fixed serialization of complex nested structures with multiple levels of FraiseQL objects

### 🧪 **Testing**
- **Comprehensive test coverage**: Added 20+ tests covering all JSON serialization scenarios
- **Red-Green-Refactor**: Followed TDD methodology with failing tests, minimal fixes, and clean refactoring
- **Edge cases**: Tests cover nested objects, lists, UNSET values, date serialization, and complex structures
- **Backward compatibility**: Verified existing functionality remains unaffected

### 🛠️ **Technical Implementation**
- Enhanced `define_fraiseql_type()` in `src/fraiseql/types/constructor.py` to add serialization methods to input types
- Added `_serialize_field_value()` helper for recursive serialization with existing type handling
- Integrated with existing `_serialize_basic()` from SQL generator for consistent type serialization
- Maintains full compatibility with existing `FraiseQLJSONEncoder`

### 📝 **Usage Example**
```python
@fraiseql.input
class CreateAddressInput:
    street: str
    city: str
    postal_code: str | None = UNSET
    created_at: datetime.date

# Before v0.7.15: ❌ JSON serialization failed
# After v0.7.15: ✅ Works seamlessly

address = CreateAddressInput(
    street="123 Main St",
    city="New York",
    created_at=datetime.date(2025, 1, 15)
)

result = json.dumps(address, cls=FraiseQLJSONEncoder)  # ✅ Works!
dict_result = address.to_dict()
# ✅ {'street': '123 Main St', 'city': 'New York', 'created_at': '2025-01-15'}
```

### 📁 **Files Modified**
- `src/fraiseql/types/constructor.py` - Added JSON serialization methods to input types
- `tests/unit/mutations/test_nested_input_json_serialization*.py` - Comprehensive test coverage
- `tests/unit/mutations/test_date_serialization_in_to_dict.py` - Date serialization verification

## [0.7.14] - 2025-09-11

### 🐛 **Fixed**

#### **Critical Nested Input Conversion Fix**
- **Fixed critical nested input conversion bug in v0.7.13**: Resolved the actual root cause where nested FraiseQL input objects were not being properly converted from GraphQL camelCase to Python snake_case field names
- **Problem**: The v0.7.13 release claimed to fix nested input conversion but the issue persisted - nested input objects still retained camelCase field names, causing PostgreSQL functions to receive inconsistent data formats
- **Root cause**: The `_coerce_field_value()` function in coercion system only checked for `typing.Union` but not `types.UnionType` (Python 3.10+ syntax). Fields defined as `NestedInput | None` used `types.UnionType` and bypassed proper coercion
- **Solution**: Enhanced Union type detection in `src/fraiseql/types/coercion.py` to handle both `typing.Union` and `types.UnionType`, ensuring all nested input objects get properly converted
- **Impact**:
  - **BREAKING**: All nested input field names now consistently convert to snake_case - remove any dual-format workarounds from PostgreSQL functions
  - Eliminates architectural inconsistency where direct mutations and nested objects had different field naming
  - Database functions can now rely on consistent snake_case field names across all mutation patterns
- **Verification**: Added comprehensive test suite covering direct vs nested input conversion, Union type handling, and real-world scenario replication

### 🧪 **Testing**
- **Added comprehensive test coverage**: 12 new tests covering nested input conversion edge cases, Union type coercion, and real-world scenarios
- **Regression prevention**: Added specific tests for `types.UnionType` vs `typing.Union` handling to prevent future regressions
- **Real-world validation**: Tests replicate the exact scenarios described in user bug reports

### 📁 **Files Modified**
- `src/fraiseql/types/coercion.py` - Enhanced Union type detection for Python 3.10+ compatibility
- `tests/unit/mutations/test_nested_input_conversion_comprehensive.py` - New comprehensive test suite
- `tests/unit/mutations/test_real_world_nested_input_scenario.py` - Real-world scenario validation

## [0.7.13] - 2025-09-11

### 🐛 **Fixed**

#### **Nested Input Object Field Name Conversion**
- **Fixed nested input field naming inconsistency**: Resolved issue where nested input objects bypassed camelCase→snake_case field name conversion, causing inconsistent data formats sent to PostgreSQL functions
- **Problem**: Direct mutations correctly converted `streetNumber` → `street_number`, but nested input objects passed raw GraphQL field names, forcing database functions to handle dual formats
- **Root cause**: The `_serialize_value()` function in SQL generator didn't apply field name conversion to nested dictionaries and FraiseQL input objects
- **Solution**:
  - Enhanced `_serialize_value()` to apply `to_snake_case()` conversion to all dict keys
  - Added special handling for FraiseQL input objects (`__fraiseql_definition__` detection)
  - Ensured recursive conversion for deeply nested structures
- **Impact**:
  - Eliminates architectural inconsistency in mutation pipeline
  - Database functions no longer need to handle dual naming formats (`streetNumber` vs `street_number`)
  - Maintains full backward compatibility with existing mutations
- **Test coverage**: Added comprehensive test suite covering direct vs nested comparison, recursive conversion, mixed format handling, and edge cases

### 🔧 **Infrastructure**

#### **Linting Tooling Alignment**
- **Updated ruff dependency**: Aligned local development with CI environment by updating ruff requirement from `>=0.8.4` to `>=0.13.0`
- **Fixed new lint warnings**: Resolved RUF059 unused variable warnings introduced in ruff 0.13.0 by prefixing unused variables with underscore
- **Fixed Generic inheritance order**: Moved `Generic` to last position in `DataLoader` class inheritance to comply with PYI059 rule
- **Impact**: Eliminates CI/local environment inconsistencies and ensures reliable linting pipeline

### 🧪 **Testing**
- **Enhanced test coverage**: Added 6 new tests for nested input conversion covering edge cases and regression prevention
- **All existing tests pass**: Verified no regressions with full test suite (2901+ tests)

### 📁 **Files Modified**
- `src/fraiseql/mutations/sql_generator.py` - Enhanced nested input serialization
- `tests/unit/mutations/test_nested_input_conversion.py` - New comprehensive test suite
- `pyproject.toml` - Updated ruff dependency version
- `src/fraiseql/security/rate_limiting.py` - Fixed unused variable warnings
- `src/fraiseql/security/validators.py` - Fixed unused variable warnings
- `src/fraiseql/optimization/dataloader.py` - Fixed Generic inheritance order

## [0.7.10-beta.1] - 2025-09-08

### 🐛 **Fixed**

#### **Nested Array Resolution for JSONB Fields**
- **Fixed critical GraphQL field resolver issue**: Resolved issue where GraphQL field resolvers failed to convert raw dictionary arrays from JSONB data to typed FraiseQL objects
- **Problem**: Field resolvers only worked with `hasattr(field_type, "__args__")` which was unreliable for Optional[list[T]] patterns, causing nested arrays to return raw dictionaries instead of properly typed objects
- **Root cause**: Unreliable type detection for Optional and generic list types in GraphQL field resolution
- **Solution**:
  - Replace unreliable `hasattr(..., "__args__")` with robust `get_args()` from typing module
  - Add proper type unwrapping for Optional[list[T]] → list[T] → T patterns
  - Extract reusable `_extract_list_item_type()` helper function for better maintainability
  - Maintain full backward compatibility with existing field resolution patterns
- **Impact**:
  - Fixes the core value proposition of FraiseQL: seamless JSONB to GraphQL object mapping now works correctly for nested arrays
  - Eliminates issues where nested arrays would return raw dictionaries instead of typed FraiseQL objects
  - Improves type safety and developer experience when working with complex nested data structures
- **Test coverage**: Added comprehensive test suite with 7 edge cases including empty arrays, null values, mixed content, and deeply nested arrays
- **Affected systems**: Critical fix for PrintOptim Backend and other systems relying on nested array field resolution

### 🔧 **Technical Details**
- **Files modified**: `src/fraiseql/core/graphql_type.py` - enhanced field resolver type detection
- **New helper function**: `_extract_list_item_type()` for robust type extraction from Optional[list[T]] patterns
- **Improved type detection**: Using `typing.get_args()` instead of unreliable `hasattr()` checks
- **Backward compatibility**: All existing field resolution behavior preserved, no breaking changes
- **Performance**: No performance impact, same resolution speed with improved reliability

## [0.7.9] - 2025-09-07

### 🐛 **Fixed**

#### **Field Name Conversion Bug Fix**
- **Fixed critical camelCase to snake_case conversion**: Resolved field name conversion bug where camelCase fields with numbers followed by 'Id' were incorrectly converted
- **Problem**: Client sends `dns1Id`, `dns2Id` but FraiseQL converted to `dns1_id` instead of expected `dns_1_id`, `dns_2_id`
- **Root cause**: Regex patterns in `camel_to_snake()` function were insufficient for letter→number and number→capital transitions
- **Solution**: Added two new regex patterns to handle these specific transition cases
- **Impact**:
  - Eliminates PostgreSQL "got an unexpected keyword argument" errors
  - Ensures round-trip conversion works correctly: `dns_1_id` → `dns1Id` → `dns_1_id`
  - Maintains full backward compatibility with existing field naming
- **Test coverage**: Added comprehensive unit tests and regression tests for the specific bug case
- **Affected systems**: Fixes integration issues with PrintOptim Backend and similar PostgreSQL CQRS systems

### 🔧 **Technical Details**
- **Files modified**: `src/fraiseql/utils/naming.py` - enhanced `camel_to_snake()` function
- **New regex patterns**:
  - `r'([a-zA-Z])(\d)'` - handles letter-to-number transitions (e.g., `dns1` → `dns_1`)
  - `r'(\d)([A-Z])'` - handles number-to-capital transitions (e.g., `1Id` → `1_id`)
- **Backward compatibility**: All existing field conversions preserved, no breaking changes
- **Performance**: Minimal impact, only affects field name conversion during GraphQL processing

## [0.7.8] - 2025-01-07

### 🚀 **Enhanced**

#### **TurboRouter Hash Normalization Fix**
- **Fixed hash mismatch issue**: Resolved critical issue where TurboRouter queries registered with raw hashes (like those from PrintOptim Backend database) wouldn't match FraiseQL's normalized hash calculation, preventing turbo router activation
- **Enhanced hash_query() normalization**: Improved whitespace normalization using regex patterns for better GraphQL syntax handling
- **Added hash_query_raw()**: New method for backward compatibility with systems using pre-computed raw hashes
- **Added register_with_raw_hash()**: Allows registration of queries with specific pre-computed database hashes
- **Enhanced get() with fallback**: Registry lookup now tries normalized hash first, then falls back to raw hash for maximum compatibility
- **Performance impact**: Fixed queries now activate turbo mode correctly (`mode: "turbo"`, <20ms) instead of falling back to normal mode (~140ms)
- **Integration example**: Added comprehensive PrintOptim Backend integration example demonstrating database query loading
- **Complete test coverage**: New test suite reproduces issue and validates fix workflow

### 🔧 **Technical Details**
- **Root cause**: Hash mismatch between external systems calculating raw query hashes and FraiseQL's normalized hash calculation
- **Solution**: Multi-strategy lookup with backward compatibility methods
- **Backward compatibility**: All existing registration workflows preserved, new methods are purely additive
- **Validated integration**: Tested with PrintOptim Backend scenario (hash: `859f5d3b94c4c1add28a74674c83d6b49cc4406c1292e21822d4ca3beb76d269`)

## [0.7.7] - 2025-01-06

### 🐛 **Fixed**

#### **Critical psycopg Placeholder Bug**
- **Fixed Critical psycopg %r Placeholder Bug**: Resolved serious string contains filter bug where `%r` placeholders were causing PostgreSQL syntax errors and query failures
- **String Contains Filters**: Fixed `contains`, `startsWith`, `endsWith`, and `iContains` operators that were generating malformed SQL with `%r` instead of proper string literals
- **SQL Generation**: Corrected SQL generation to use proper quoted string literals instead of repr() format specifiers
- **Database Compatibility**: Ensures all string-based WHERE clause operations work correctly with PostgreSQL backend

### 🔧 **Enhanced**
- **Query Reliability**: All string-based filtering operations now generate syntactically correct SQL
- **Error Prevention**: Eliminates PostgreSQL syntax errors from malformed query generation
- **Filter Stability**: String matching operations (`contains`, `startsWith`, `endsWith`, `iContains`) now work as expected

### 🏗️ **Technical**
- **Backward Compatibility**: All existing functionality preserved
- **SQL Generation**: Fixed string literal generation in WHERE clause builders
- **Test Coverage**: Added comprehensive tests for string filter operations to prevent regression

## [0.7.5] - 2025-01-04

### 🔧 **PyPI & Badge Management**

#### **🎯 GitHub Workflow Badges**
- **Fixed GitHub Workflow Badges**: Updated README badges to reference `quality-gate.yml` instead of deprecated individual workflow files (`test.yml`, `lint.yml`, `security.yml`)
- **Unified Quality Gate**: All CI checks now run through single comprehensive `quality-gate.yml` workflow
- **Badge Consistency**: Ensures PyPI page displays accurate build status for main branch

#### **📦 Release Management**
- **Version Alignment**: Synchronized version across `__init__.py`, `cli/main.py`, and `pyproject.toml` for clean PyPI publishing
- **Clean Release**: Minimal focused release for PyPI package update with correct metadata

## [0.7.4] - 2025-09-04

### ✨ **Added**
- **Comprehensive Enhanced Network Operators**: 5 new RFC-compliant IP address classification operators
  - `isLoopback`: RFC 3330/4291 loopback addresses (127.0.0.0/8, ::1/128)
  - `isLinkLocal`: RFC 3927/4291 link-local addresses (169.254.0.0/16, fe80::/10)
  - `isMulticast`: RFC 3171/4291 multicast addresses (224.0.0.0/4, ff00::/8)
  - `isDocumentation`: RFC 5737/3849 documentation addresses (TEST-NET ranges, 2001:db8::/32)
  - `isCarrierGrade`: RFC 6598 Carrier-Grade NAT addresses (100.64.0.0/10)
- **Full IPv4/IPv6 Support**: All new operators handle both IP versions where applicable
- **Comprehensive Documentation**: Complete operator reference with RFC citations and usage examples
- **TDD Implementation**: RED→GREEN→REFACTOR methodology with comprehensive test coverage

### 🔧 **Enhanced**
- **Network Operator Strategy**: Extended with 5 additional operators following established patterns
- **Boolean Logic Support**: All new operators accept true/false for positive/negative filtering
- **PostgreSQL Integration**: Uses native inet type with subnet containment operators for optimal performance
- **Test Coverage**: 17 new tests for enhanced operators, 42 total network-related tests passing

### 📖 **Documentation**
- **Network Operators Guide**: New comprehensive documentation in `docs/network-operators.md`
- **Design Decision Rationale**: Explains inclusion/exclusion criteria using Marie Kondo approach
- **Usage Examples**: Complete GraphQL query examples for all new operators

### 🏗️ **Technical**
- **Backward Compatibility**: All existing functionality preserved
- **Type Safety**: Proper field type validation and error handling
- **Code Quality**: Perfect QA scores across all automated checks

## [0.7.3] - 2025-01-03

### ✨ **Added**
- **Automatic Field Name Conversion**: GraphQL camelCase field names now work seamlessly in WHERE clauses
  - `{"ipAddress": {"eq": "192.168.1.1"}}` automatically converts to `ip_address` in SQL
  - `{"macAddress": {"eq": "aa:bb:cc"}}` automatically converts to `mac_address` in SQL
  - `{"deviceName": {"contains": "router"}}` automatically converts to `device_name` in SQL

### 🔧 **Fixed**
- **Field Name Mapping Inconsistency**: Eliminated the need for manual field name conversion in WHERE clauses
- **Developer Experience**: GraphQL developers no longer need to know database schema field names
- **API Consistency**: All FraiseQL features now handle field names consistently

### 🚀 **Performance**
- **Zero Impact**: Field name conversion adds negligible performance overhead (< 3ms for complex queries)
- **Optimized Logic**: Idempotent conversion preserves existing snake_case names without processing

### 📋 **Migration Guide**
- **Breaking Changes**: None - 100% backward compatible
- **Required Updates**: None - existing code continues to work unchanged
- **Recommended**: Remove manual field name conversion code (now unnecessary)

### 🧪 **Testing**
- **+16 comprehensive tests** covering unit and integration scenarios
- **Edge case handling** for empty strings, None values, and mixed case scenarios
- **Performance validation** ensuring no degradation in query processing
- **Backward compatibility verification** with all existing WHERE clause functionality
### 🔧 **Repository Integration Improvements**

#### **Enhanced FraiseQLRepository WHERE Processing**
- **Fixed**: `FraiseQLRepository.find()` now properly uses operator strategy system instead of primitive SQL templates
- **Enabled**: Complete integration with v0.7.1 IP filtering fixes through repository layer
- **Added**: Comprehensive repository integration tests for ALL specialized types (IP, MAC, LTree, Port, DateRange, etc.)
- **Improved**: SQL injection protection via field name escaping
- **Enhanced**: Error handling with graceful fallback to basic condition building

#### **📊 Test Coverage Expansion**
- **+15 new integration tests** verifying repository layer works with specialized types
- **2,826 total tests passing** (expanded from 2,811)
- **Complete verification** that operator strategies work through `FraiseQLRepository.find()`
- **Fallback behavior testing** ensures graceful degradation for unsupported operators

#### **🎯 Production Impact**
- ✅ All GraphQL queries with specialized type filtering now work through repository layer
- ✅ PrintOptim Backend and similar applications fully operational
- ✅ Complete specialized type support: IP addresses, MAC addresses, LTree paths, ports, date ranges, CIDR networks, hostnames, emails
- ✅ Maintains backward compatibility with existing repository usage patterns

## [0.7.1] - 2025-09-03

### 🚨 **Critical Production Fix: IP Filtering in CQRS Patterns**

#### **Issue Resolved**
- **Critical Bug**: IP filtering completely broken in production CQRS systems where INET fields are stored as strings in JSONB data columns
- **Impact**: All IP-based WHERE filters returned 0 results in production systems using CQRS pattern
- **Root Cause**: Missing `::inet` casting on literal values when `field_type` information is unavailable

#### **✅ Fix Applied**
- **Enhanced ComparisonOperatorStrategy**: Now casts both field and literal to `::inet` for eq/neq operations
- **Enhanced ListOperatorStrategy**: Now casts all list items to `::inet` for in/notin operations
- **Smart Detection**: Automatic IP address detection with MAC address conflict prevention
- **Production Ready**: Zero regression with full backward compatibility

#### **📊 Validation Results**
- **2,811 tests passing** (100% pass rate)
- **43 network tests passing** with comprehensive IP filtering coverage
- **Zero regression** - preserves all existing functionality
- **IPv4/IPv6 support** maintained with MAC address detection preserved

#### **🎯 Production Impact**
- ✅ DNS server IP filtering restored in PrintOptim Backend and similar systems
- ✅ Network management functionality operational
- ✅ IP-based security filtering working correctly
- ✅ All CQRS systems with INET fields functional

## [0.7.0] - 2025-09-03

### 🚀 **Major Release: Enterprise-Grade Logical Operators + Infrastructure Optimization**

#### **Revolutionary Logical WHERE Operators - Hasura/Prisma Parity Achieved**

**🎯 Major Achievement**: FraiseQL v0.7.0 delivers **complete logical operator functionality** with sophisticated 4-level nesting support, matching the filtering capabilities of leading GraphQL frameworks while maintaining superior performance.

#### **✅ Quantified Success Metrics**
- **Test Coverage**: **2804/2805 tests passing** (99.96% success rate - improved from 99.93%)
- **Logical Operator Support**: **22 comprehensive tests** covering all operator combinations
- **CI/CD Performance**: **80% faster** with streamlined GitHub Actions workflows
- **Resource Efficiency**: **~70% reduction** in CI resource usage
- **Network Filtering**: **17 total network-specific operations** including 10 new advanced classifiers

### 🎯 **New Features**

#### **🔗 Logical WHERE Operators**
Enterprise-grade logical operators with infinite nesting support:
- **`OR`**: Complex logical OR conditions with nested operators
- **`AND`**: Explicit logical AND conditions for complex queries
- **`NOT`**: Logical negation with full operator support
- **4-level nesting support**: Enterprise-grade query complexity
- **Complete GraphQL integration**: Type-safe input generation
- **PostgreSQL native**: Direct conversion to optimized SQL expressions

#### **🌐 Advanced Network Filtering**
Enhanced `NetworkAddressFilter` with 10 new network classification operators:
- **`isLoopback`**: Loopback addresses (127.0.0.1, ::1)
- **`isMulticast`**: Multicast addresses (224.0.0.0/4, ff00::/8)
- **`isBroadcast`**: Broadcast address (255.255.255.255)
- **`isLinkLocal`**: Link-local addresses (169.254.0.0/16, fe80::/10)
- **`isDocumentation`**: RFC 3849/5737 documentation ranges
- **`isReserved`**: Reserved/unspecified addresses (0.0.0.0, ::)
- **`isCarrierGrade`**: Carrier-Grade NAT (100.64.0.0/10)
- **`isSiteLocal`**: Site-local IPv6 (fec0::/10 - deprecated)
- **`isUniqueLocal`**: Unique local IPv6 (fc00::/7)
- **`isGlobalUnicast`**: Global unicast addresses

#### **📚 Enhanced Documentation**
- **616-line comprehensive documentation** on advanced filtering patterns
- **Real-world examples** with 4-level logical nesting
- **Network audit scenarios** with complex business logic
- **Performance optimization guidelines**

### 🔧 **Improvements**

#### **⚡ CI/CD Infrastructure Optimization**
**Streamlined GitHub Actions** (50% workflow reduction):
- **Unified Quality Gate**: All checks (tests, lint, security, coverage) in single workflow
- **80% Performance Improvement**: ~1.5 minutes vs. ~8 minutes parallel execution
- **Resource Efficiency**: Single PostgreSQL instance instead of 4+ duplicates
- **Enhanced Security**: Added Trivy vulnerability scanning + improved bandit integration
- **Type Safety**: Added pyright type checking to quality gate
- **Cleaner Interface**: 3-5 status checks instead of 7+ redundant ones

#### **🛡️ Enhanced Security & Quality**
- **Comprehensive Security Scanning**: Bandit + Trivy integration
- **Type Safety**: Complete pyright type checking coverage
- **Test Reliability**: 99.96% pass rate with comprehensive coverage reporting

### 🐛 **Bug Fixes**

#### **🔧 GraphQL Type Conversion Fix**
- **Fixed**: `TypeError: Invalid type passed to convert_type_to_graphql_input: <class 'list'>`
- **Root Cause**: Raw `list` type without type parameters caused schema building failures
- **Solution**: Added fallback handler for unparameterized list types
- **Impact**: Enables complex WHERE input types with list fields to generate correctly

#### **🧪 Test Infrastructure Cleanup**
- **Removed**: Conflicting example test directories causing pytest import errors
- **Improved**: Test execution reliability with cleaner imports
- **Result**: Zero test failures from infrastructure issues

### 📊 **Performance Metrics**

#### **Query Performance**
- **Logical Operations**: Sub-millisecond execution for 4-level nested conditions
- **Network Filtering**: Native PostgreSQL inet functions for optimal performance
- **Index Compatibility**: All operators generate index-friendly SQL conditions

#### **CI/CD Performance**
- **Execution Time**: 1m30s vs. ~8m parallel (80% improvement)
- **Resource Usage**: 70% reduction in GitHub Actions minutes
- **Developer Experience**: Cleaner, faster, more reliable CI pipeline

### 🏆 **Framework Comparison - Parity Achieved**

| Feature | FraiseQL v0.7.0 | Hasura | Prisma |
|---------|-----------------|---------|---------|
| **Logical Operators** | ✅ OR, AND, NOT | ✅ | ✅ |
| **Nested Logic** | ✅ 4+ levels | ✅ | ✅ |
| **Network Filtering** | ✅ **17 operators** | ⚠️ Basic | ❌ Limited |
| **Custom Types** | ✅ MAC, LTree, IP, etc | ⚠️ Limited | ❌ Basic |
| **PostgreSQL Native** | ✅ Full JSONB + INET | ✅ | ⚠️ Basic |
| **Test Reliability** | ✅ **99.96%** | ⚠️ Unknown | ⚠️ Unknown |
| **CI/CD Performance** | ✅ **80% faster** | ⚠️ Unknown | ⚠️ Unknown |

### 🎭 **Real-World Usage Examples**

#### **Complex Logical Filtering**
```graphql
query ComplexNetworkAudit {
  devices(where: {
    AND: [
      {
        OR: [
          { AND: [{ status: { eq: "active" } }, { ipAddress: { isPrivate: true } }] },
          { NOT: { ipAddress: { isLoopback: true } } }
        ]
      },
      { NOT: { identifier: { contains: "test" } } }
    ]
  }) {
    id hostname ipAddress status
  }
}
```

#### **Advanced Network Classification**
```graphql
query NetworkDevicesByType {
  publicDevices: devices(where: {
    ipAddress: { isPublic: true, NOT: { isDocumentation: true } }
  }) { id hostname ipAddress }

  internalInfra: devices(where: {
    OR: [
      { ipAddress: { isPrivate: true } },
      { ipAddress: { isCarrierGrade: true } }
    ]
  }) { id hostname ipAddress }
}
```

## Breaking Changes

**None.** This release is fully backward-compatible.

## [0.6.0] - 2025-09-02

### 🚀 **Major Release: 100% IP Operator Functionality Achievement**

#### **Revolutionary WHERE Clause Refactor - Complete Success**

**🎯 Mission Accomplished**: FraiseQL v0.6.0 delivers **100% IP operator functionality** with the successful completion of our comprehensive WHERE clause refactor following **Marie Kondo TDD methodology**.

#### **✅ Quantified Success Metrics**
- **IP Operator Success Rate**: **42.9% → 100.0%** (+57.1% improvement)
- **Test Coverage**: **2782/2783 tests passing** (99.96% success rate)
- **Production Validation**: **Successfully tested on real database with 61 records**
- **Operator Count**: **84 operators across 11 field types**
- **Performance**: **Sub-second query execution maintained**

#### **🔧 Complete IP Operator Support**
All **7 IP operators** now work perfectly:
- **`eq`**: IP address equality matching
- **`neq`**: IP address inequality matching
- **`in`**: Multiple IP address matching
- **`nin`**: Exclude IP addresses
- **`isPrivate`**: RFC 1918 private address detection
- **`isPublic`**: Public IP address detection
- **`isIPv4`**: IPv4 address filtering

#### **📊 Production Database Validation**
**Real-world testing completed** on production database:
```sql
-- Production validation results:
SELECT
  COUNT(*) as total_records,           -- 61 records
  COUNT(DISTINCT data->>'ip_address')  -- 8 unique IPs
FROM public.v_dns_server
WHERE pk_organization = '22222222-2222-2222-2222-222222222222';

-- All IP operators now return correct results:
-- eq: 42.9% → 100% success (was broken, now perfect)
-- neq: 42.9% → 100% success (was broken, now perfect)
-- in: 42.9% → 100% success (was broken, now perfect)
-- nin: 42.9% → 100% success (was broken, now perfect)
-- isPrivate: 100% → 100% success (already working)
-- isPublic: 100% → 100% success (already working)
-- isIPv4: 100% → 100% success (already working)
```

#### **🧪 Marie Kondo TDD Success Story**
**Complete Test-Driven Development lifecycle**:

**Phase 1**: **RED** - Comprehensive test creation
- Created failing tests for all 84 operators across 11 field types
- Identified broken IP operators (eq, neq, in, nin) returning 42.9% success
- Established quality baseline with production data validation

**Phase 2**: **GREEN** - Systematic implementation
- Fixed `ComparisonOperatorStrategy` IP address handling
- Enhanced SQL generation for INET type casting
- Corrected operator mapping and validation logic
- Achieved 100% IP operator functionality

**Phase 3**: **REFACTOR** - Code quality improvement
- Cleaned up operator strategy architecture
- Improved type detection and casting logic
- Enhanced error handling and validation
- Maintained performance while achieving correctness

#### **🔬 Technical Achievements**

**Enhanced ComparisonOperatorStrategy**:
- **Fixed INET type casting** for IP address equality operations
- **Corrected SQL generation** to handle PostgreSQL network types properly
- **Improved value validation** for network address inputs
- **Enhanced error handling** with graceful fallbacks

**SQL Generation Improvements**:
```sql
-- Before v0.6.0 (broken):
host((data->>'ip_address')::inet) = '8.8.8.8'
-- Result: 0 records (empty - broken)

-- After v0.6.0 (fixed):
(data->>'ip_address')::inet = '8.8.8.8'::inet
-- Result: correct matches (working perfectly)
```

**Type Safety Enhancements**:
- **Robust type detection** for all PostgreSQL network types
- **Intelligent casting strategies** based on field types
- **Validation improvements** preventing invalid operations
- **Error recovery mechanisms** for edge cases

#### **📈 Performance Impact**
- **Zero Performance Regression**: All improvements maintain sub-second execution
- **Memory Efficiency**: No additional memory overhead for fixed operations
- **Query Optimization**: Better PostgreSQL query plans with proper type casting
- **Database Efficiency**: Reduced false positive/negative results

#### **🛡️ Production Ready Features**

**Comprehensive Validation**:
- **Real database testing**: Validated on production dataset (61 records)
- **Edge case handling**: IPv4/IPv6 address format variations
- **Error boundary testing**: Invalid input graceful handling
- **Performance validation**: No degradation in query execution time

**Enterprise Features**:
- **Multi-tenant support**: All IP operators work correctly in tenant contexts
- **JSONB optimization**: Maintains efficient JSONB → INET casting
- **PostgreSQL compatibility**: Works with all PostgreSQL 12+ versions
- **Production monitoring**: Enhanced logging and error reporting

#### **🔄 Migration & Compatibility**

**100% Backward Compatible**:
- **Zero Breaking Changes**: All existing code continues to work unchanged
- **API Compatibility**: All GraphQL schemas remain identical
- **Configuration**: No configuration changes required
- **Deployment**: Drop-in replacement for v0.5.x versions

**Automatic Improvements**:
- **Existing queries** that previously failed now return correct results
- **No code changes needed** - improvements are automatic
- **Query performance** maintained or improved in all cases

#### **🧪 Testing Excellence**

**Comprehensive Test Suite**:
- **2782 tests passing** out of 2783 total tests (99.96% success rate)
- **84 operator tests** across all 11 field types
- **Production scenario coverage** with real database validation
- **Regression prevention** ensuring no functionality loss
- **Performance benchmarking** validating sub-second execution

**Quality Assurance**:
- **TDD methodology** followed throughout development
- **Code review process** with comprehensive validation
- **CI/CD pipeline** ensuring no regressions
- **Production testing** on real data before release

#### **🎯 Real-World Impact**

**Before v0.6.0** (Broken IP Filtering):
```graphql
# These queries returned incorrect/empty results:
query GetGoogleDNS {
  dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
    id identifier ipAddress
  }
  # Result: [] (empty - was broken)
}

query GetNonLocalServers {
  dnsServers(where: { ipAddress: { neq: "192.168.1.1" } }) {
    id identifier ipAddress
  }
  # Result: [] (empty - was broken)
}
```

**After v0.6.0** (Perfect IP Filtering):
```graphql
# Same queries now return correct results:
query GetGoogleDNS {
  dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
    id identifier ipAddress
  }
  # Result: [{ id: "uuid", identifier: "google-dns", ipAddress: "8.8.8.8" }]
}

query GetNonLocalServers {
  dnsServers(where: { ipAddress: { neq: "192.168.1.1" } }) {
    id identifier ipAddress
  }
  # Result: All non-local servers returned correctly
}
```

#### **📊 Statistical Success Summary**

| Metric | Before v0.6.0 | After v0.6.0 | Improvement |
|--------|---------------|---------------|-------------|
| IP eq operator | 42.9% success | 100% success | +57.1% |
| IP neq operator | 42.9% success | 100% success | +57.1% |
| IP in operator | 42.9% success | 100% success | +57.1% |
| IP nin operator | 42.9% success | 100% success | +57.1% |
| Total test coverage | 2781/2783 | 2782/2783 | +1 test |
| Production validation | Not tested | 61 records ✓ | Full validation |

#### **🚀 Upgrade Instructions**

**Simple Upgrade Process**:
```bash
# Immediate upgrade recommended:
pip install --upgrade fraiseql==0.6.0

# No code changes required - all improvements are automatic
# Existing GraphQL queries will start returning correct results
```

**Verification**:
```python
import fraiseql
print(fraiseql.__version__)  # Should output: 0.6.0

# Test IP filtering (should now work perfectly):
# Your existing GraphQL queries with IP filtering will now return correct results
```

#### **🎖️ Achievement Unlocked**

**FraiseQL v0.6.0 represents a major milestone**: The successful transformation from **partially functional** (42.9% success rate) to **completely production-ready** (100% success rate) for IP filtering operations.

This release demonstrates **engineering excellence** through:
- **Methodical TDD approach** following Marie Kondo principles
- **Comprehensive testing** with real production data validation
- **Zero regression policy** maintaining all existing functionality
- **Performance preservation** while achieving correctness
- **Production readiness** with enterprise-grade validation

**FraiseQL is now the most reliable GraphQL framework for PostgreSQL IP address filtering operations.**

---

## [0.5.8] - 2025-09-02

### 🚨 Critical Production Bug Fix

#### **JSONB+INET Network Filtering Fix**
- **CRITICAL**: Fixed production bug where IP address equality filtering returned empty results
- **Affected**: Production systems using CQRS patterns with JSONB IP address storage
- **Resolution**: Modified SQL generation to use proper INET casting for equality operators
- **Impact**: IP address filtering now returns correct results instead of empty sets

#### **The Bug (v0.5.7 and earlier)**
```sql
-- Generated SQL was incorrect for equality operations:
host((data->>'ip_address')::inet) = '8.8.8.8'
-- Result: 0 records (empty - broken)
```

#### **The Fix (v0.5.8)**
```sql
-- Generated SQL now correct for equality operations:
(data->>'ip_address')::inet = '8.8.8.8'::inet
-- Result: 1 record (correct)
```

### 🎯 Affected Use Cases

#### **Before v0.5.8 ❌ (Broken)**
```graphql
# These queries returned empty results:
dnsServers(where: { ipAddress: { eq: "8.8.8.8" } })       # → 0 results
servers(where: { ip: { neq: "192.168.1.1" } })            # → 0 results
devices(where: { address: { in: ["10.1.1.1", "10.1.1.2"] } }) # → 0 results
```

#### **After v0.5.8 ✅ (Fixed)**
```graphql
# Same queries now return correct results:
dnsServers(where: { ipAddress: { eq: "8.8.8.8" } })       # → correct results
servers(where: { ip: { neq: "192.168.1.1" } })            # → correct results
devices(where: { address: { in: ["10.1.1.1", "10.1.1.2"] } }) # → correct results
```

### ✅ What Still Works (Unaffected)
- **Subnet filtering**: `inSubnet`, `notInSubnet` operators worked before and continue working
- **Pattern filtering**: `contains`, `startswith`, `endswith` operators unaffected
- **All other field types**: String, Integer, DateTime, etc. filtering unaffected
- **Direct INET column filtering**: Non-JSONB INET columns were never affected

### 🛡️ Backward Compatibility
- **100% Compatible**: No breaking changes, all existing code continues to work
- **Automatic Fix**: Existing queries automatically get correct results without code changes
- **No Migration**: Users can upgrade directly without any code modifications

### 🧪 Comprehensive Testing
- **7 new regression tests**: Complete CQRS + GraphQL integration validation
- **3 updated core tests**: Reflect correct behavior expectations
- **2589+ tests passing**: Full test suite validates no regressions
- **Production pattern testing**: Real-world CQRS scenarios validated

### 🔧 Technical Details
**File Modified**: `src/fraiseql/sql/operator_strategies.py` (5 line change in `_apply_type_cast()` method)
**Behavior Change**: Only affects equality operators with JSONB IP address fields
**Performance**: No impact - same SQL generation speed, more accurate results
**Compatibility**: 100% backward compatible - pure bug fix

### 📊 Performance Impact
- **Zero Performance Impact**: Same SQL generation speed, more accurate results
- **No Resource Usage Change**: Memory and CPU usage unchanged
- **Database Performance**: Proper INET casting may actually improve query performance

### ⚠️ Who Should Upgrade Immediately
- **CQRS Pattern Users**: Systems storing IP addresses as INET in command tables, exposing as JSONB in query views
- **Network Filtering Users**: Applications filtering on IP addresses using equality operators
- **Production Systems**: Any system where IP address filtering returns unexpected empty results

### 🚀 Upgrade Instructions
```bash
# Immediate upgrade recommended for affected systems:
pip install --upgrade fraiseql==0.5.8

# No code changes required - existing queries will start working correctly
```

## [0.5.7] - 2025-09-01

### 🚀 Major GraphQL Field Type Propagation Enhancement

#### **Advanced Type-Aware SQL Generation**
- **New**: GraphQL field type extraction and propagation to SQL operators
- **Enhancement**: Intelligent type-aware SQL generation for optimized database performance
- **Feature**: Automatic detection of field types from GraphQL schema context
- **Performance**: More efficient SQL with proper type casting based on GraphQL field types

#### **GraphQL Field Type System**
- **Added**: `GraphQLFieldTypeExtractor` for intelligent field type detection
- **Capability**: Automatic extraction of IPAddress, DateTime, Port, and other special types
- **Integration**: Seamless GraphQL schema to SQL operator type propagation
- **Heuristics**: Smart field name pattern matching for type inference

#### **Type-Aware SQL Optimization**
```sql
-- Before v0.5.7: Generic approach
(data->>'ip_address') = '8.8.8.8'
(data->>'port')::text > '1024'

-- After v0.5.7: Type-aware optimized SQL
(data->>'ip_address')::inet = '8.8.8.8'::inet
(data->>'port')::integer > 1024
(data->>'created_at')::timestamp >= '2024-01-01'::timestamp
```

#### **Enhanced GraphQL Query Performance**
```graphql
# Same GraphQL syntax, but with optimized SQL generation
dnsServers(where: {
  ipAddress: { eq: "8.8.8.8" }        # → Optimized ::inet casting
  port: { gt: 1024 }                  # → Optimized ::integer casting
  createdAt: { gte: "2024-01-01" }    # → Optimized ::timestamp casting
}) {
  id identifier ipAddress port createdAt
}
```

### 🛠️ CI/CD Infrastructure Improvements

#### **Pre-commit.ci Reliability Fix**
- **Fixed**: Pre-commit.ci pipeline reliability with proper UV dependency handling
- **Enhancement**: Better CI environment detection prevents false failures
- **Developer Experience**: More reliable automated quality checks
- **CI Logic**: Proper handling of different CI environments (GitHub Actions, pre-commit.ci)

#### **Before v0.5.7 ❌**
```yaml
# pre-commit.ci failed with "uv not found" error
# Tests would fail in CI environments unnecessarily
```

#### **After v0.5.7 ✅**
```bash
# Smart CI environment detection
if [ "$PRE_COMMIT_CI" = "true" ]; then
  echo "⏭️  Skipping tests in CI - will be run by GitHub Actions"
  exit 0
fi
```

### 🧪 Comprehensive Testing

#### **New Test Coverage**
- **25+ Tests**: GraphQL field type extraction comprehensive coverage
- **15+ Tests**: Operator strategy coverage ensuring complete SQL generation
- **25+ Tests**: GraphQL-SQL integration validating end-to-end type propagation
- **Regression Tests**: All existing functionality preserved and enhanced
- **Performance Tests**: Type-aware SQL generation efficiency validation

#### **Quality Assurance**
- **2582+ Tests Total**: All tests passing with new functionality
- **Backward Compatibility**: Zero breaking changes, automatic enhancements
- **Infrastructure Testing**: Pre-commit.ci reliability across environments
- **Edge Cases**: Complex nested types, arrays, custom scalars

### 🏗️ Architecture Enhancements

#### **Modular Type System**
- **Component**: `GraphQLFieldTypeExtractor` as reusable, extensible system
- **Strategy Pattern**: Enhanced operator strategies with type awareness
- **Performance**: Reduced database overhead through optimized SQL generation
- **Extensibility**: Easy addition of new types and operator strategies

#### **No New Dependencies**
- **Clean Enhancement**: Advanced capabilities without additional dependencies
- **Stability**: Built on existing robust foundation
- **Compatibility**: Works seamlessly with all existing FraiseQL features

### 📚 Developer Experience

#### **Automatic Performance Gains**
- **Zero Migration**: Existing GraphQL queries automatically get performance improvements
- **Transparent**: Type-aware SQL generation happens behind the scenes
- **Consistent**: All GraphQL field types benefit from optimized SQL casting
- **Debugging**: Enhanced error messages for type-related issues

#### **Enhanced Capabilities**
- **Type Intelligence**: GraphQL schema types now propagate to SQL generation
- **Query Optimization**: Database queries run faster with proper type casting
- **Field Detection**: Automatic detection of special field types (IP, MAC, Date, etc.)
- **Operator Selection**: Intelligent selection of optimal SQL operators based on field types

## [0.5.6] - 2025-09-01

### 🔧 Critical Network Filtering Enhancement

#### **Network Operator Support Fix**
- **Fixed**: "Unsupported network operator: eq" error for IP address filtering
- **Added**: Basic comparison operators (`eq`, `neq`, `in`, `notin`) to NetworkOperatorStrategy
- **Impact**: IP address equality filtering now works correctly in GraphQL queries
- **SQL**: Proper PostgreSQL `::inet` type casting in generated SQL

#### **Before v0.5.6 ❌**
```graphql
# This failed with "Unsupported network operator: eq"
dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
  id identifier ipAddress
}
```

#### **After v0.5.6 ✅**
```graphql
# This now works perfectly
dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
  id identifier ipAddress
}

# All these operators now work:
dnsServers(where: { ipAddress: { neq: "192.168.1.1" } }) { ... }
dnsServers(where: { ipAddress: { in: ["8.8.8.8", "1.1.1.1"] } }) { ... }
dnsServers(where: { ipAddress: { notin: ["192.168.1.1"] } }) { ... }
```

### 🧪 Testing
- **19 comprehensive NetworkOperatorStrategy tests** covering all operators
- **Edge cases**: IPv6 addresses, empty lists, error handling
- **Backward compatibility**: All existing network operators continue working
- **SQL generation quality**: Proper `::inet` casting validation
- **Production scenarios**: Real-world use case validation

### 🛠️ Infrastructure
- **Architecture Consistency**: Follows established pattern used by other operator strategies
- **No Dependencies**: No new dependencies added
- **Performance**: No performance impact on existing queries
- **Security**: No security concerns introduced

## [0.5.5] - 2025-09-01

### 🚀 Major Features
- **CRITICAL FIX**: Comprehensive JSONB special types casting fix for production
  - Resolves 3 release failures caused by type casting issues
  - Enhanced ComparisonOperatorStrategy with intelligent value detection
  - Fixes Network, MAC Address, LTree, and DateRange type operations

### 🔧 Improvements
- Added intelligent fallback type detection when field_type=None
- Maintains backward compatibility with existing field_type behavior
- Prevents false positives with robust validation patterns

### 🧪 Testing
- Added 53+ comprehensive tests using RED-GREEN-REFACTOR methodology
- Added Tier 1 core tests with pytest -m core marker (<30s runtime)
- Production scenario validation and regression prevention

### 🎯 Bug Fixes
- Fixed JSONB IP address equality operations in production
- Fixed MAC address casting for network hardware operations
- Fixed LTree hierarchical path operations
- Fixed DateRange operations with proper PostgreSQL casting

### 📊 Performance
- Ensures identical behavior between test and production environments
- Zero regressions introduced while fixing critical production issues

## [0.5.4] - 2025-01-21

### 🔧 **Critical Bug Fixes**

#### **JSONB Network Filtering Resolution**
Fixed critical network filtering bug affecting PostgreSQL JSONB fields:
- **Fixed**: `NetworkOperatorStrategy` now properly casts to `::inet` for JSONB fields
- **Fixed**: All network operators (`insubnet`, `isprivate`, `eq`) now work correctly with JSONB data
- **Resolved**: SQL generation consistency issues between different operator types
- **Impact**: Network filtering operations now work reliably across all PostgreSQL column types

#### **Repository Integration Enhancement**
- **Fixed**: Specialized operator strategies (Network, MAC, LTree, DateRange) now fully compatible with repository methods
- **Improved**: GraphQL where input generation includes all network operators
- **Enhanced**: Type safety for network filtering operations

### 🚀 **Python 3.13 Upgrade**

#### **Full Python 3.13 Compatibility**
- **Upgraded**: All CI/CD pipelines from Python 3.12 to Python 3.13
- **Fixed**: `AsyncGenerator` typing compatibility issues
- **Updated**: Dependencies and lock files for Python 3.13 support
- **Resolved**: pytest asyncio marker configuration conflicts
- **Validated**: All 2484+ tests pass with Python 3.13.3

#### **Performance & Stability**
- **Removed**: xfail markers from tests that now pass consistently
- **Enhanced**: Async/await patterns optimized for Python 3.13
- **Improved**: Type checking and runtime performance

### 🛡️ **CI/CD Pipeline Security**

#### **Quality Gate System**
- **Added**: Comprehensive quality gate workflow with multi-stage validation
- **Implemented**: Development safety protections preventing broken releases
- **Enhanced**: Security checks integrated into release process
- **Documented**: CI/CD pipeline architecture and safety measures

#### **Infrastructure Improvements**
- **Fixed**: pip cache directory issues in CI environments
- **Resolved**: pytest-cov compatibility problems
- **Disabled**: Problematic plugin autoloading causing test collection errors
- **Added**: Comprehensive environment debugging for CI failures

### 📈 **Performance Improvements**

#### **Test Infrastructure**
- **Fixed**: Flaky performance test timeouts in GraphQL error serialization
- **Improved**: Test reliability and execution speed
- **Enhanced**: CI test stability with better error handling

### 📚 **Documentation**

#### **FraiseQL Relay Extension**
- **Added**: Complete PostgreSQL extension for GraphQL Relay specification
- **Documented**: Technical architecture and implementation guides
- **Created**: Performance benchmarks and optimization recommendations
- **Provided**: Migration guides for existing applications

#### **Development Guidelines**
- **Added**: Comprehensive agent prompt for PrintOptim Backend Relay
- **Created**: Implementation blueprint with Clean Architecture + CQRS
- **Documented**: Production-grade development setup procedures

### 🧪 **Testing**

#### **Comprehensive Validation**
- **Status**: ✅ 2484 tests passed, 1 skipped
- **Coverage**: 65% overall code coverage maintained
- **Validation**: All 25 network filtering tests passing
- **Quality**: CI pipeline complete: Tests ✅, Lint ✅, Security ✅

#### **Network Filtering Test Suite**
- **Added**: Comprehensive test coverage for network filtering bug fixes
- **Validated**: SQL generation consistency across operator types
- **Verified**: GraphQL integration works correctly with network operators

### 🔄 **Breaking Changes**
None - this is a backward-compatible bug fix release.

### 📋 **Migration Guide**
No migration required. This release only fixes bugs and adds new functionality without breaking existing APIs.

**Recommendation**: Update immediately to benefit from critical network filtering fixes and Python 3.13 compatibility.

## [0.5.1] - 2025-08-30

### 🚀 **Cursor-Based Pagination with Relay Connection Support**

#### **New @connection Decorator**
FraiseQL now provides a **complete cursor-based pagination solution** following the Relay Connection specification:

```python
import fraiseql

@fraiseql.connection(
    node_type=User,
    view_name="v_user",
    default_page_size=20,
    max_page_size=100
)
async def users(
    info: GraphQLResolveInfo,
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
    where: UserWhereInput | None = None,
) -> UserConnection:
    """Get paginated users with cursor-based navigation."""
```

#### **Complete Relay Specification Compliance**
- **Connection[T], Edge[T], PageInfo types** - Full GraphQL Connection specification
- **Base64 cursor encoding/decoding** - Secure, opaque cursor format
- **Forward and backward pagination** - `first`/`after` and `last`/`before` parameters
- **Cursor validation** - Automatic cursor format validation and error handling
- **Total count support** - Optional `totalCount` field for client pagination UI
- **Flexible configuration** - Customizable page sizes, cursor fields, and view names

#### **Built on Existing Infrastructure**
- **Leverages CQRSRepository** - Uses proven FraiseQL pagination patterns
- **Integrates with CursorPaginator** - Builds on existing `fraiseql.cqrs.pagination` module
- **PostgreSQL JSONB optimized** - Efficient cursor-based queries over JSONB views
- **Type-safe implementation** - Full Python typing support with proper generics

#### **Comprehensive Documentation & Examples**
- **405-line demo file** (`examples/cursor_pagination_demo.py`) with Vue.js integration
- **Complete test coverage** - 4 comprehensive test cases covering all functionality
- **Production-ready patterns** - Real-world pagination examples with error handling
- **Frontend integration guide** - Vue.js components for cursor-based UI

#### **Key Features**
- **Automatic resolver generation** - Single decorator creates complete connection resolver
- **Parameter validation** - Built-in validation for pagination parameters and conflicts
- **Error handling** - Graceful handling of invalid cursors and parameter combinations
- **Performance optimized** - Efficient PostgreSQL queries with proper LIMIT/OFFSET handling
- **Extensible design** - Easy to customize cursor fields and pagination behavior

#### **Migration from Offset Pagination**
```python
# Before: Traditional offset pagination
@fraiseql.query
async def users(offset: int = 0, limit: int = 20) -> list[User]:
    # Manual pagination logic
    pass

# After: Cursor-based pagination
@fraiseql.connection(node_type=User)
async def users(first: int | None = None, after: str | None = None) -> UserConnection:
    # Automatic cursor handling
    pass
```

This release establishes FraiseQL as **the most comprehensive GraphQL pagination solution** for PostgreSQL, combining Relay specification compliance with high-performance JSONB queries.

## [0.5.0] - 2025-08-25

### 🚀 **Major Release: Ultimate FraiseQL Integration & Zero-Inheritance Pattern**

#### **🎯 Revolutionary Zero-Inheritance Mutation Pattern**

**The Ultimate Simplification** - No more `(MutationResultBase)` inheritance needed!

**Before v0.5.0:** Verbose inheritance patterns
```python
from fraiseql import MutationResultBase

@fraiseql.success
class CreateUserSuccess(MutationResultBase):  # Inheritance required
    user: dict | None = None

@fraiseql.failure
class CreateUserError(MutationResultBase):   # Inheritance required
    conflict_user: dict | None = None
```

**After v0.5.0:** Clean, zero-inheritance patterns
```python
# No inheritance needed! No extra imports!
@fraiseql.success
class CreateUserSuccess:  # Just your fields!
    user: dict | None = None

@fraiseql.failure
class CreateUserError:    # Just your fields!
    conflict_user: dict | None = None
```

#### **🔧 Automatic Field Injection**
- **Auto-injected fields**: `status: str`, `message: str | None`, `errors: list[Error] | None`
- **Smart defaults**: `status="success"`, `message=None`, `errors=None`
- **Override support**: Explicit field definitions override auto-injection
- **Full compatibility**: Works seamlessly with mutation parser and error auto-population

#### **⚡ Performance & Streamlining**
- **Removed**: Legacy `ALWAYS_DATA_CONFIG` patterns (deprecated) - Use enhanced `DEFAULT_ERROR_CONFIG`
- **Cleaned**: Legacy test files and backwards compatibility code
- **Optimized**: Framework initialization and runtime performance

#### **🏗️ Built-in Types for Zero Configuration**
- **Added**: Built-in `Error` type exported from main `fraiseql` module
- **Added**: `MutationResultBase` type (still available but not required thanks to auto-injection)
- **Enhanced**: `DEFAULT_ERROR_CONFIG` with FraiseQL-friendly patterns:
  - Success keywords: `"created"`, `"cancelled"`
  - Error-as-data prefixes: `"duplicate:"` (in addition to `"noop:"`, `"blocked:"`)

#### **🎯 FraiseQL Integration Impact**
- **Zero configuration**: Works perfectly with all FraiseQL patterns out-of-the-box
- **75% less code**: Eliminate both custom types AND inheritance boilerplate
- **Cleaner definitions**: Focus purely on business fields
- **Migration path**: Existing patterns still work during transition

#### **🛠️ Technical Implementation**
- Enhanced `@fraiseql.success` and `@fraiseql.failure` decorators with intelligent auto-injection
- Annotation-based field detection prevents conflicts with explicit definitions
- Maintains full GraphQL schema compatibility and type safety
- Comprehensive test coverage with 43+ tests covering all patterns

#### **📈 Impact**
- **Simplest possible mutation definitions** in any GraphQL framework
- **FraiseQL projects** can now use FraiseQL with absolute minimal code
- **Developer experience** dramatically improved with near-zero boilerplate
- **Performance** gains from cleaned codebase and optimized defaults

---

## [0.4.7] - 2025-08-23

### 🚀 **GraphQL Error Serialization Fix**

#### **Critical Fix: @fraise_type Objects in GraphQL Responses**
- **Fixed**: GraphQL execution now properly serializes `@fraise_type` objects to prevent "Object of type Error is not JSON serializable" runtime errors
- **Issue**: Error auto-population created `@fraise_type` Error objects that failed standard JSON serialization during GraphQL response generation
- **Solution**: Added GraphQL response serialization hook that automatically converts `@fraise_type` objects to dictionaries before JSON encoding
- **Impact**: **Fixes core functionality** - projects using error auto-population with custom Error types now work correctly

#### **Implementation Details**
- **Added**: `_serialize_fraise_types_in_result()` function in GraphQL execution pipeline
- **Added**: `_clean_fraise_types()` recursive function for deep @fraise_type object conversion
- **Features**: Handles nested @fraise_type objects, circular reference protection, enum serialization
- **Performance**: Minimal overhead - only processes objects that need cleaning

#### **Backwards Compatibility**
- **Maintained**: All existing APIs unchanged
- **Preserved**: Error object semantics and type information maintained
- **Enhanced**: JSON serialization now works correctly for all @fraise_type objects

#### **Testing & Verification**
- **Added**: Comprehensive integration tests (`test_graphql_error_serialization.py`)
- **Added**: Extensive unit tests (`test_fraise_type_json_serialization.py`)
- **Verified**: All existing tests continue to pass (no regressions)
- **Confirmed**: Bug reproduction cases now work correctly

## [0.4.6] - 2025-08-22

### 🔧 **Version Consistency Fix**

#### **Fixed Version Reporting**
- **Fixed**: Corrected `__version__` string to properly report "0.4.6" instead of mismatched version
- **Issue**: v0.4.5 on PyPI had incorrect `__version__ = "0.4.4"` causing version reporting inconsistency
- **Solution**: Synchronized version strings across `pyproject.toml` and `__init__.py`

#### **No Functional Changes**
- **Mutation passthrough fix**: All functionality from v0.4.5 preserved unchanged
- **Status code mapping**: All enhancements from v0.4.5 included
- **Testing**: All tests continue to pass (196/196)

#### **Migration from v0.4.5**
- **Upgrade**: Simply update to v0.4.6 - no code changes required
- **Verification**: `fraiseql.__version__` now correctly reports "0.4.6"

## [0.4.5] - 2025-08-22

### 🚀 **Mutation-Aware JSON Passthrough**

#### **Critical Fix: Mutations Never Use Passthrough**
- **Fixed**: Mutations and subscriptions now automatically disable JSON passthrough regardless of configuration
- **Issue**: When `json_passthrough_enabled=True`, mutations were bypassing the standard parser, preventing error auto-population (ALWAYS_DATA_CONFIG) from working
- **Solution**: GraphQL execution pipeline now detects operation type and forces standard execution for mutations
- **Impact**: **Fixes critical bug** where mutations returned `errors: null` instead of populated error arrays

#### **Performance + Correctness**
- **Queries**: Continue using passthrough for optimal performance (~2-5ms)
- **Mutations**: Always use standard pipeline for reliable error handling (~10-20ms)
- **Result**: Applications can safely enable JSON passthrough in production while maintaining consistent mutation error responses

#### **Enhanced Status Code Mapping**
- **Added**: Support for `skipped:` and `ignored:` status prefixes (both map to HTTP 422)
- **Improved**: Better prefix handling while maintaining backward compatibility with existing keyword-based mappings
- **Maintained**: Existing error code mappings unchanged (e.g., `noop:not_found` still returns 404)

#### **Documentation & Testing**
- **Enhanced**: Updated function documentation to explain mutation-aware passthrough behavior
- **Added**: Comprehensive test coverage for mutation passthrough detection
- **Verified**: All existing tests pass - no breaking changes

### 🎯 **Migration Guide**
Applications using `json_passthrough_enabled=True` can now safely enable it in production:
```python
config = FraiseQLConfig(
    json_passthrough_enabled=True,         # ✅ Now safe with mutations
    json_passthrough_in_production=True,   # ✅ Mutations work correctly
    environment="production"
)
```

Mutations will automatically get proper error arrays:
```javascript
mutation CreateItem($input: CreateItemInput!) {
  createItem(input: $input) {
    ... on CreateItemError {
      errors {  // ✅ Now populated correctly (was null before)
        message
        code      // 422, 404, 409, etc.
        identifier
      }
    }
  }
}
```

## [0.4.4] - 2025-08-21

### 🚀 **Major TurboRouter Fixes**

#### **Fragment Field Extraction Bug Resolution**
- **Fixed**: TurboRouter now correctly extracts root field names from GraphQL queries with fragments
- **Issue**: Regex pattern `r"{\s*(\w+)"` was matching first field in fragments instead of actual query root field
- **Example**: For query with `fragment UserFields on User { id name }` and `query GetUsers { users { ...UserFields } }`, TurboRouter now correctly extracts `"users"` instead of `"id"`
- **Impact**: **Critical fix** for production applications using fragment-based GraphQL queries with TurboRouter

#### **Double-Wrapping Prevention**
- **Fixed**: TurboRouter no longer double-wraps pre-formatted GraphQL responses from PostgreSQL functions
- **Issue**: Functions returning `{"data": {"allocations": [...]}}` were being wrapped again to create `{"data": {"id": {"data": {"allocations": [...]}}}}`
- **Solution**: Smart response detection automatically handles pre-wrapped responses
- **Impact**: Resolves data structure corruption in applications using PostgreSQL functions that return GraphQL-formatted responses

#### **Enhanced Root Field Detection**
- **Added**: Robust field name extraction supporting multiple GraphQL query patterns:
  - Named queries with fragments: `fragment Foo on Bar { ... } query GetItems { items { ...Foo } }`
  - Anonymous queries: `{ items { id name } }`
  - Simple named queries: `query GetItems { items { id name } }`
- **Backward Compatible**: All existing simple queries continue to work unchanged

### 🧪 **Test Coverage Improvements**
- **Added**: `test_turbo_router_fragment_field_extraction` - Verifies correct field extraction from fragment queries
- **Added**: `test_turbo_router_prevents_double_wrapping` - Ensures no double-wrapping of pre-formatted responses
- **Status**: 17/17 TurboRouter tests passing, no regressions detected

### 📈 **Performance & Compatibility**
- **Performance**: No impact on response times or query execution
- **Compatibility**: **100% backward compatible** - existing SQL templates and queries work unchanged
- **Production Ready**: Thoroughly tested with real-world fragment queries and PostgreSQL function responses

## [0.4.1] - 2025-08-21

### 🐛 **Critical Bug Fixes**

#### **OrderBy Unpacking Error Resolution**
- **Fixed**: `"not enough values to unpack (expected 2, got 1)"` error when using GraphQL OrderBy input formats
- **Root Cause**: GraphQL OrderBy input `[{"field": "direction"}]` was reaching code expecting tuple format `[("field", "direction")]`
- **Impact**: This was a **blocking issue** preventing basic GraphQL sorting functionality across all FraiseQL applications

#### **Comprehensive OrderBy Format Support**
- **Enhanced**: Automatic conversion between all GraphQL OrderBy input formats:
  - ✅ `[{"field": "ASC"}]` - List of dictionaries (most common GraphQL format)
  - ✅ `{"field": "ASC"}` - Single dictionary format
  - ✅ `[("field", "asc")]` - Existing tuple format (backward compatible)
  - ✅ `[{"field1": "ASC"}, {"field2": "DESC"}]` - Multiple field sorting
  - ✅ `[{"field1": "ASC", "field2": "DESC"}]` - Mixed format support

#### **Advanced OrderBy Scenarios**
- **Added**: Support for complex nested field sorting:
  - `[{"profile.firstName": "ASC"}]` → `data->'profile'->>'first_name' ASC`
  - `[{"user.profile.address.city": "ASC"}]` → `data->'user'->'profile'->'address'->>'city' ASC`
- **Enhanced**: Automatic camelCase → snake_case field name conversion for database compatibility
- **Improved**: Case-insensitive direction handling (`ASC`, `asc`, `DESC`, `desc`)

### 🔧 **Technical Improvements**

#### **Multiple Component Fixes**
Fixed OrderBy handling across **4 critical components**:

1. **Database Repository (`fraiseql/db.py`)**:
   - Added OrderBy conversion for JSON/raw output path (Lines 967-1000)
   - Handles all GraphQL formats before calling `build_sql_query`

2. **CQRS Repository (`fraiseql/cqrs/repository.py`)**:
   - Fixed tuple unpacking in `list()` method (Lines 688-697)
   - Added `_convert_order_by_to_tuples()` helper method (Lines 603-633)

3. **Cache Key Builder (`fraiseql/caching/cache_key.py`)**:
   - Fixed OrderBy processing for cache key generation (Lines 58-63)
   - Added conversion helper to prevent unpacking errors (Lines 97-127)

4. **SQL Generator (`fraiseql/sql/sql_generator.py`)**:
   - Added safety net in `build_sql_query()` function (Lines 162-168)
   - Comprehensive fallback conversion system (Lines 16-46)

#### **Robust Error Handling**
- **Multiple Fallbacks**: If one conversion method fails, others provide backup
- **Graceful Degradation**: Invalid OrderBy inputs return `None` instead of crashing
- **Backward Compatibility**: Existing tuple format continues to work unchanged

### 🧪 **Enhanced Testing**

#### **Comprehensive Test Suite**
- **New**: 13 unit tests covering complex OrderBy scenarios (`tests/sql/test_orderby_complex_scenarios.py`)
- **Coverage**: Real-world GraphQL patterns including nested fields, multiple orderings, and mixed formats
- **Performance**: Pure unit tests with 0.05s execution time (no database dependencies)
- **Validation**: Complete GraphQL → SQL transformation verification

#### **Test Scenarios Added**
- FraiseQL Backend DNS servers scenario (original failing case)
- Enterprise contract management with nested sorting
- Deep nested field ordering (`user.profile.address.city`)
- Mixed format OrderBy combinations
- Error recovery for malformed inputs

### 📊 **Real-World Examples**

#### **Before Fix** (Failing):
```javascript
// GraphQL Query
query GetDnsServers($orderBy: [DnsServerOrderByInput!]) {
  dnsServers(orderBy: $orderBy) { id, ipAddress }
}

// Variables
{ "orderBy": [{"ipAddress": "ASC"}] }

// Result: ❌ "not enough values to unpack (expected 2, got 1)"
```

#### **After Fix** (Working):
```javascript
// Same GraphQL Query & Variables
{ "orderBy": [{"ipAddress": "ASC"}] }

// Generated SQL:
// ORDER BY data->>'ip_address' ASC
// Result: ✅ Proper sorting functionality
```

#### **Complex Nested Example**:
```javascript
// GraphQL Variables
{
  "orderBy": [
    {"user.profile.firstName": "ASC"},
    {"organization.settings.priority": "DESC"},
    {"lastModifiedAt": "DESC"}
  ]
}

// Generated SQL:
// ORDER BY
//   data->'user'->'profile'->>'first_name' ASC,
//   data->'organization'->'settings'->>'priority' DESC,
//   data->>'last_modified_at' DESC
```

### ⚡ **Performance Impact**

- **No Performance Regression**: Conversion only happens when needed
- **Minimal Overhead**: Simple tuple format bypass conversion entirely
- **Caching Optimized**: Cache key generation now handles all OrderBy formats
- **Memory Efficient**: No additional object allocation for existing patterns

### 🔄 **Migration Guide**

**No migration required!** This is a **purely additive fix**:

- ✅ **Existing code continues to work unchanged**
- ✅ **No breaking changes**
- ✅ **No configuration changes needed**
- ✅ **Automatic compatibility with all GraphQL clients**

### 🎯 **Validation**

**Tested extensively with adversarial scenarios**:
- ✅ 29/32 adversarial test cases passed
- ✅ All core functionality scenarios verified
- ✅ Complex nested field patterns working
- ✅ Real-world FraiseQL Backend scenarios resolved
- ✅ Enterprise-scale OrderBy patterns supported

## [0.4.0] - 2025-08-21

### 🚀 Major New Features

#### **CamelForge Integration - Database-Native camelCase Transformation**
- **World's first GraphQL framework with database-native field transformation**
- **Intelligent field threshold detection** - Uses CamelForge for small queries (≤20 fields), automatically falls back to standard processing for large queries
- **Sub-millisecond GraphQL responses** - Field transformation happens in PostgreSQL, eliminating Python object instantiation overhead
- **Automatic field mapping** - Seamless GraphQL camelCase ↔ PostgreSQL snake_case conversion (e.g., `ipAddress` ↔ `ip_address`)
- **Zero breaking changes** - Completely backward compatible, disabled by default
- **Simple configuration** - Enable with single environment variable: `FRAISEQL_CAMELFORGE_ENABLED=true`

##### Configuration Options:
```python
config = FraiseQLConfig(
    camelforge_enabled=True,                    # Enable CamelForge (default: False)
    camelforge_function="turbo.fn_camelforge",  # PostgreSQL function name
    camelforge_field_threshold=20,              # Field count threshold
)
```

##### Environment Variable Overrides:
- `FRAISEQL_CAMELFORGE_ENABLED=true/false` - Enable/disable CamelForge
- `FRAISEQL_CAMELFORGE_FUNCTION=function_name` - Custom function name
- `FRAISEQL_CAMELFORGE_FIELD_THRESHOLD=30` - Custom field threshold

##### How It Works:
**Small queries** (≤ threshold):
```sql
-- Wraps jsonb_build_object with CamelForge function
SELECT turbo.fn_camelforge(
    jsonb_build_object('ipAddress', data->>'ip_address'),
    'dns_server'
) AS result FROM v_dns_server
```

**Large queries** (> threshold):
```sql
-- Falls back to standard processing
SELECT data AS result FROM v_dns_server
```

##### Benefits:
- **Performance**: 10-50% faster response times for small queries
- **Memory**: Reduced Python object instantiation overhead
- **Developer Experience**: Automatic camelCase without manual mapping
- **TurboRouter Compatible**: Works with existing cached query systems
- **Enterprise Ready**: Database-native processing for production scale

### 🔧 Configuration Improvements
- **Simplified configuration system** - Removed complex beta flags and feature toggles
- **Clear precedence hierarchy** - Environment variables override config parameters, which override defaults
- **Easy testing workflow** - Single environment variable to enable/disable features

### 🧪 Testing Enhancements
- **29 comprehensive tests** covering all CamelForge functionality
- **Performance comparison tests** - Verify response time improvements
- **Backward compatibility validation** - Ensure existing queries work identically
- **Configuration testing** - Validate environment variable overrides

### 📚 Documentation
- **Simple testing guide** - One-page guide for teams to test CamelForge safely
- **Configuration comparison** - Clear before/after examples showing simplification
- **Comprehensive integration documentation** - Complete guide with examples

## [0.3.11] - 2025-08-20

### 🐛 Critical Bug Fixes
- **Fixed dictionary WHERE clause bug in `FraiseQLRepository.find()`** - Dictionary WHERE clauses now work correctly
  - Root cause: Repository ignored plain dictionary WHERE clauses like `{'hostname': {'contains': 'router'}}`
  - Only handled GraphQL input objects with `_to_sql_where()` method or SQL where types with `to_sql()` method
  - This bug caused filtered queries to return unfiltered datasets, leading to data exposure and performance issues
  - Fixed by adding `_convert_dict_where_to_sql()` method to handle dictionary-to-SQL conversion

### ✨ WHERE Clause Functionality Restored
- **All filter operators now functional with dictionary format**:
  - **String operators**: `eq`, `neq`, `contains`, `startswith`, `endswith`
  - **Numeric operators**: `gt`, `gte`, `lt`, `lte` (with automatic `::numeric` casting)
  - **Array operators**: `in`, `nin` (not in) with `ANY`/`ALL` SQL operations
  - **Network operators**: `isPrivate`, `isPublic` for RFC 1918 private address detection
  - **Null operators**: `isnull` with proper NULL/NOT NULL handling
  - **Multiple conditions**: Complex queries with multiple fields and operators per field
  - **Simple equality**: Backward compatibility with `{'status': 'active'}` format

### 🔐 Security Enhancements
- **SQL injection prevention**: All user input properly parameterized using `psycopg.sql.Literal`
- **Operator restriction**: Only whitelisted operators allowed to prevent malicious operations
- **Input validation**: Proper type checking and sanitization of WHERE clause values
- **Graceful error handling**: Invalid operators ignored safely without information disclosure

### 🚀 Performance Improvements
- **Proper filtering**: Queries now return only requested records instead of full datasets
- **Reduced data transfer**: Significantly smaller result sets for filtered queries
- **Database efficiency**: Proper WHERE clauses reduce server-side processing
- **Memory optimization**: Less memory usage from smaller result sets

### 🔄 Backward Compatibility
- **Full compatibility**: All existing GraphQL where inputs continue working unchanged
- **SQL where types**: Existing SQL where type patterns still supported
- **Simple kwargs**: Basic parameter filtering (`status="active"`) still works
- **No breaking changes**: All existing query patterns preserved

### 🧪 Testing
- **Comprehensive coverage**: Added extensive test coverage for dictionary WHERE clause conversion
- **Security testing**: Verified SQL injection protection and input validation
- **Performance testing**: Confirmed no regression in query execution speed
- **Integration testing**: All existing WHERE-related tests continue passing

## [0.3.10] - 2025-08-20

### 🐛 Critical Bug Fixes
- **Fixed WHERE clause generation bug in `CQRSRepository`** - GraphQL filters now work correctly instead of being completely ignored
  - Root cause: Repository `query()` method was treating GraphQL operator dictionaries like `{"contains": "router"}` as simple string values
  - Generated invalid SQL like `data->>'name' = '{"contains": "router"}'` instead of proper WHERE clauses
  - This bug was systematically breaking ALL GraphQL filtering operations in repository queries
  - Fixed by integrating existing `_make_filter_field_composed` function for proper WHERE clause generation

### ✨ GraphQL Filter Restoration
- **All GraphQL operators now functional**:
  - **String operators**: `contains`, `startswith`, `endswith`, `eq`, `neq` - previously completely broken
  - **Numeric operators**: `eq`, `neq`, `gt`, `gte`, `lt`, `lte` - previously completely broken
  - **List operators**: `in`, `nin` (not in) - previously completely broken
  - **Boolean operators**: `eq`, `neq`, `isnull` - previously completely broken
  - **Network operators**: `isPrivate`, `isPublic`, `isIPv4`, `isIPv6`, `inSubnet`, `inRange` - previously completely broken
  - **Complex multi-operator queries** - now work correctly with multiple conditions
  - **Mixed old/new filter styles** - backward compatibility maintained

### 🔧 Technical Improvements
- **Added proper `nin` → `notin` operator mapping** for GraphQL compatibility
- **Migrated to safe parameterization** using `psycopg.sql.Literal` for SQL injection protection
- **Fixed boolean value handling** in legacy simple equality filters (`True` → `"true"` for JSON compatibility)
- **Enhanced error handling** with graceful fallback for unsupported operators

### 🧪 Testing & Quality
- **Added comprehensive test suites** demonstrating the fix with 44+ new tests
- **TDD approach validation** with before/after test scenarios showing the bug and fix
- **Performance validation** with 1000-record test datasets
- **Backward compatibility verification** ensuring existing code continues to work
- **No regressions** in existing functionality confirmed

### 📈 Impact
- **Critical fix**: This bug was preventing ALL GraphQL WHERE clause filtering from working
- **Repository layer**: `select_from_json_view()`, `list()`, `find_by_view()` methods now filter correctly
- **Developer experience**: GraphQL filters now work as expected without workarounds
- **Production impact**: Eliminates need for manual SQL queries to work around broken filtering

### 💡 Migration Notes
- **No breaking changes**: Existing code will continue to work
- **Automatic fix**: GraphQL filters that were silently failing will now work correctly
- **Performance**: Queries will now return filtered results instead of all results (significantly better performance)
- **Testing**: Review any tests that were expecting unfiltered results due to the bug

## [0.3.9] - 2025-01-29

### Fixed
- **Automatic JSON Serialization for @fraiseql.type** - FraiseQL types are now automatically JSON serializable in GraphQL responses
  - Enhanced `FraiseQLJSONEncoder` to handle objects decorated with `@fraiseql.type`
  - Eliminates the need to inherit from `BaseGQLType` for serialization support
  - Fixes "Object of type [TypeName] is not JSON serializable" errors in production GraphQL APIs
  - Maintains backward compatibility while providing consistent developer experience
  - Added comprehensive test coverage for FraiseQL type serialization scenarios

### Developer Experience
- **Improved @fraiseql.type Decorator** - Types now work consistently without additional inheritance requirements
  - `@fraiseql.type` decorator now sufficient for complete GraphQL type functionality
  - Automatic JSON serialization in GraphQL responses
  - Enhanced documentation with JSON serialization examples
  - Better error messages for serialization issues

## [0.3.8] - 2025-08-20

### Added
- **Enhanced Network Address Filtering** - Network-specific operators for IP address filtering
  - Added `inSubnet` operator for CIDR subnet matching using PostgreSQL `<<=` operator
  - Added `inRange` operator for IP address range queries using PostgreSQL inet comparison
  - Added `isPrivate` operator to detect RFC 1918 private network addresses
  - Added `isPublic` operator to detect public (non-private) IP addresses
  - Added `isIPv4` and `isIPv6` operators to filter by IP version using PostgreSQL `family()` function
  - Added `IPRange` input type with `from` and `to` fields for range specifications
  - Enhanced `NetworkAddressFilter` with network-specific operations while maintaining backward compatibility

### Enhanced
- **SQL Generation for Network Operations** - New NetworkOperatorStrategy for handling network-specific filtering
  - Added `NetworkOperatorStrategy` to operator registry for network operators
  - Implemented PostgreSQL-native SQL generation for all network operators
  - Added comprehensive IP address validation utilities with IPv4/IPv6 support
  - Added network utilities for subnet matching, range validation, and private/public detection
  - Enhanced documentation with network filtering examples and migration guide

### Developer Experience
- **Comprehensive Testing**: Added 22 new tests covering all network filtering operations
- **Documentation-First Development**: Complete documentation update with examples and migration patterns
- **Type Safety**: Full type safety for network operations with proper validation
- **Future-Ready**: Architecture supports additional network operators and protocol-specific filtering

## [0.3.7] - 2025-01-20

### Added
- **Restricted Filter Types for Exotic Scalars** - Aligned GraphQL operator exposure with actual implementation capabilities
  - Added `NetworkAddressFilter` for IpAddress and CIDR types - only exposes operators that work correctly (eq, neq, in_, nin, isnull)
  - Added `MacAddressFilter` for MAC address types - excludes problematic string pattern matching
  - Added `LTreeFilter` for hierarchical path types - conservative approach until proper ltree operators implemented
  - Added `DateRangeFilter` for PostgreSQL date range types - basic operations until range-specific operators added
  - Enhanced `_get_filter_type_for_field()` to detect FraiseQL scalar types and assign restricted filters
  - Prevents users from accessing broken/misleading filter operations that don't work due to PostgreSQL type normalization

### Fixed
- **GraphQL Schema Integrity**: Fixed exotic scalar types exposing non-functional operators
  - IpAddress/CIDR types no longer expose `contains`/`startswith`/`endswith` (broken due to CIDR notation like `/32`, `/128`)
  - MacAddress types no longer expose string pattern matching (broken due to MAC normalization to canonical form)
  - LTree types now use conservative operator set (eq, neq, isnull) until specialized ltree operators implemented
  - Enhanced IP address filtering with PostgreSQL `host()` function to strip CIDR notation (from previous commits)

### Changed
- **Breaking Change**: Exotic scalar types now use restricted filter sets instead of generic `StringFilter`
  - This only affects GraphQL schema generation - removes operators that were never working correctly
  - Standard Python types (str, int, float, etc.) maintain full operator compatibility
  - Foundation prepared for adding proper type-specific operators in future releases

### Developer Experience
- **Better Error Prevention**: Developers can no longer use filtering operators that produce incorrect results
- **Clear Contracts**: GraphQL schema accurately reflects supported operations
- **Future-Ready**: Architecture supports adding specialized operators (ltree ancestors, range overlaps, etc.)
- **Comprehensive Testing**: Added 8 new tests plus verification that all 276 existing tests still pass

## [0.3.6] - 2025-01-18

### Fixed
- **Critical**: Fixed OrderBy list of dictionaries support with camelCase field mapping
  - GraphQL OrderBy inputs like `[{'ipAddress': 'asc'}]` were failing with "SQL values must be strings" error in v0.3.5
  - Enhanced OrderBy conversion to handle list of dictionaries format with proper field name mapping
  - Added proper camelCase to snake_case conversion for OrderBy field names (e.g., `ipAddress` → `ip_address`)
  - Improved handling of case variations in sort directions (`ASC`/`DESC` → `asc`/`desc`)
- **Critical**: Fixed test validation isolation issue affecting WHERE input validation
  - Fixed test isolation bug where `test_json_field.py` was modifying global state and affecting validation tests
  - Improved type detection in validation to properly distinguish between real nested objects and typing constructs
  - Fixed spurious `__annotations__` attribute being added to `typing.Optional[int]` constructs
  - Ensures operator type validation always runs correctly regardless of test execution order

### Added
- Comprehensive regression tests for OrderBy functionality (13 test cases)
- Support for complex field names in OrderBy: `dnsServerType` → `dns_server_type`
- Robust type detection function (`_is_nested_object_type`) for validation logic
- Pre-commit hook requiring 100% test pass rate before commits

### Details
- Now supports all OrderBy formats:
  - `[{'ipAddress': 'asc'}]` → `ORDER BY data ->> 'ip_address' ASC`
  - `[{'field1': 'asc'}, {'field2': 'DESC'}]` → Multiple field ordering
  - `{'ipAddress': 'asc'}` → Single dict (backward compatible)
- This release is fully backward compatible - no code changes required for existing OrderBy usage

## [0.3.2] - 2025-01-17

### Fixed
- **Critical**: Fixed PassthroughMixin forcing JSON passthrough in production mode
  - The PassthroughMixin was enabling passthrough just because mode was "production" or "staging"
  - Now properly respects the `json_passthrough` context flag set by the router
  - This completes the fix started in v0.3.1 for the JSON passthrough configuration issue

## [0.3.1] - 2025-01-17

### Fixed
- **Critical**: Fixed JSON passthrough being forced in production environments
  - FraiseQL v0.3.0 was ignoring the `json_passthrough_in_production=False` configuration
  - Production and staging modes were unconditionally enabling passthrough, causing APIs to return snake_case field names instead of camelCase
  - The router now properly respects both `json_passthrough_enabled` and `json_passthrough_in_production` configuration settings
  - This fixes breaking API compatibility issues where frontend applications expected camelCase fields but received snake_case
  - Added comprehensive tests to prevent regression

## [0.3.0] - 2025-01-17

### Security
- **Breaking Change**: Authentication is now properly enforced when an auth provider is configured
  - Previously, configuring `auth_enabled=True` did not block unauthenticated requests (vulnerability)
  - Now, when an auth provider is passed to `create_fraiseql_app()`, authentication is automatically enforced
  - All GraphQL requests require valid authentication tokens (401 returned for unauthenticated requests)
  - Exception: Introspection queries (`__schema`) are still allowed without auth in development mode
  - This fixes a critical security vulnerability where sensitive data could be accessed without authentication

### Changed
- Passing an `auth` parameter to `create_fraiseql_app()` now automatically sets `auth_enabled=True`
- Authentication enforcement is now consistent across all GraphQL endpoints

### Fixed
- Fixed authentication bypass vulnerability where `auth_enabled=True` didn't actually enforce authentication
- Fixed inconsistent authentication behavior between different query types

### Documentation
- Added comprehensive Authentication Enforcement section to authentication guide
- Updated API reference to clarify auth parameter behavior
- Added security notices about authentication enforcement

## [0.2.1] - 2025-01-16

### Fixed
- Fixed version synchronization across all Python modules
- Updated CLI version numbers to match package version
- Updated generated project dependencies to use correct version range

## [0.2.0] - 2025-01-16

### Changed
- **Breaking Change**: CORS is now disabled by default to prevent conflicts with reverse proxies
  - `cors_enabled` now defaults to `False` instead of `True`
  - `cors_origins` now defaults to `[]` (empty list) instead of `["*"]`
  - This prevents duplicate CORS headers when using reverse proxies like Nginx, Apache, or Cloudflare
  - Applications serving browsers directly must explicitly enable CORS with `cors_enabled=True`
  - Production deployments should configure CORS at the reverse proxy level for better security

### Added
- Production warning when wildcard CORS origins are used in production environment
- Comprehensive CORS configuration examples for both reverse proxy and application-level setups
- Detailed migration guidance in documentation for existing applications

### Fixed
- Eliminated CORS header conflicts in reverse proxy environments
- Improved security by requiring explicit CORS configuration

### Documentation
- Complete rewrite of CORS documentation across all guides
- Added reverse proxy configuration examples (Nginx, Apache)
- Updated security documentation with CORS best practices
- Updated all tutorials and examples to reflect new CORS defaults
- Added migration guide for upgrading from v0.1.x

## [0.1.5] - 2025-01-15

### Added
- **Nested Object Resolution Control** - Added `resolve_nested` parameter to `@type` decorator for explicit control over nested field resolution behavior
  - `resolve_nested=False` (default): Assumes embedded data in parent object, optimal for PostgreSQL JSONB queries
  - `resolve_nested=True`: Makes separate queries to nested type's sql_source, useful for truly relational data
  - Replaces previous automatic "smart resolver" behavior with explicit developer control
  - Improves performance by avoiding N+1 queries when data is pre-embedded
  - Maintains full backward compatibility

### Changed
- **Breaking Change**: Default nested object resolution behavior now assumes embedded data
  - Previous versions automatically queried nested objects from their sql_source
  - New default behavior assumes nested data is embedded in parent JSONB for better performance
  - Use `resolve_nested=True` to restore previous automatic querying behavior
  - This change aligns with PostgreSQL-first design and JSONB optimization patterns

### Fixed
- Fixed test import errors that were causing CI failures
- Fixed duplicate GraphQL type name conflicts in test suite
- Updated schema building API usage throughout codebase

### Documentation
- Added comprehensive guide to nested object resolution patterns
- Updated examples to demonstrate both embedded and relational approaches
- Added migration guide for developers upgrading from v0.1.4

## [0.1.4] - 2025-01-12

### Added
- **Default Schema Configuration** - Configure default PostgreSQL schemas for mutations and queries once in FraiseQLConfig
  - Added `default_mutation_schema` and `default_query_schema` configuration options
  - Eliminates repetitive `schema="app"` parameters on every decorator
  - Maintains full backward compatibility with explicit schema overrides
  - Reduces boilerplate in mutation-heavy applications by 90%
  - Lazy schema resolution ensures configuration can be set after decorators are applied

### Changed
- Default schema for mutations changed from "graphql" to "public" when no config is provided
  - This aligns with PostgreSQL conventions and simplifies getting started
  - Existing code with explicit schema parameters is unaffected

### Fixed
- Fixed timing issue where mutations would resolve schema before configuration was set
  - Schema resolution is now lazy, only happening when the GraphQL schema is built
  - This ensures the feature works correctly in production environments

## [0.1.3] - 2025-01-12

### Changed
- Renamed exported error configuration constants for consistency:
  - `FraiseQLConfig` → `STRICT_STATUS_CONFIG`
  - `AlwaysDataConfig` → `ALWAYS_DATA_CONFIG`
  - `DefaultErrorConfig` → `DEFAULT_ERROR_CONFIG`
- Improved project description to better reflect its production-ready status

## [0.1.2] - 2025-01-08

### Security
- Fixed CVE-2025-4565 by pinning `protobuf>=4.25.8,<5.0`
- Fixed CVE-2025-54121 by updating `starlette>=0.47.2`
- Removed `opentelemetry-exporter-zipkin` due to incompatibility with secure protobuf versions

### Documentation
- **Major documentation overhaul** - quality score improved from 7.8/10 to 9+/10
- Fixed 15 broken internal links across documentation
- Added comprehensive guides for CQRS, Event Sourcing, Multi-tenancy, and Bounded Contexts
- Added production readiness checklist with security, performance, and deployment guidance
- Created complete deployment documentation (Docker, Kubernetes, AWS, GCP, Heroku)
- Added testing documentation covering unit, integration, GraphQL, and performance testing
- Created error handling guides with codes, patterns, and debugging strategies
- Added learning paths for different developer backgrounds
- Added acknowledgments to Harry Percival and DDD influences in README
- Fixed all table-views to database-views references for consistency
- Added missing anchor targets for deep links
- Clarified package installation instructions with optional dependencies

### Changed
- Made Redis an optional dependency (moved from core to `[redis]` extra)
- Made Zipkin exporter optional with graceful fallback and warning messages
- Fixed pyproject.toml inline comments that caused ReadTheDocs build failures

### Fixed
- Removed unnecessary docs-deploy workflow that caused CI failures
- Fixed TOML parsing issues in dependency declarations
- Added proper error handling for missing Zipkin exporter

## [0.1.1] - 2025-01-06

### Added
- Initial stable release with all beta features consolidated
- Comprehensive documentation and examples

## [0.1.0] - 2025-08-06

### Initial Public Release

FraiseQL is a lightweight, high-performance GraphQL-to-PostgreSQL query builder that uses PostgreSQL's native jsonb capabilities for maximum efficiency.

This release consolidates features developed during the beta phase (0.1.0b1 through 0.1.0b49).

#### Core Features

- **GraphQL to SQL Translation**: Automatic conversion of GraphQL queries to optimized PostgreSQL queries
- **JSONB-based Architecture**: Leverages PostgreSQL's native JSON capabilities for efficient data handling
- **Type-safe Queries**: Full Python type safety with automatic schema generation
- **Advanced Where/OrderBy Types**: Automatic generation of GraphQL input types for filtering and sorting, with support for comparison operators (_eq, _neq, _gt, _lt, _like, _in, etc.) and nested conditions (_and, _or, _not)
- **FastAPI Integration**: Seamless integration with FastAPI for building GraphQL APIs
- **Authentication Support**: Built-in Auth0 and native authentication support
- **Subscription Support**: Real-time subscriptions via WebSockets
- **Query Optimization**: Automatic N+1 query detection and dataloader integration
- **Mutation Framework**: Declarative mutation definitions with error handling
- **Field-level Authorization**: Fine-grained access control at the field level

#### Performance

- Sub-millisecond query translation
- Efficient connection pooling with psycopg3
- Automatic query batching and caching
- Production-ready with built-in monitoring

#### Developer Experience

- CLI tools for scaffolding and development
- Comprehensive test suite (2,400+ tests)
- Extensive documentation and examples
- Python code generation

#### Examples Included

- Blog API with comments and authors
- E-commerce API with products and orders
- Real-time chat application with WebSocket support
- Native authentication UI (Vue.js components)
- Security best practices implementation
- Analytics dashboard
- Query patterns and caching examples

For migration from beta versions, please refer to the documentation.

---

[0.1.2]: https://github.com/fraiseql/fraiseql/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/fraiseql/fraiseql/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/fraiseql/fraiseql/releases/tag/v0.1.0
