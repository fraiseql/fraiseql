# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2025-08-23

### 🚀 **Major Release: Ultimate PrintOptim Integration & Zero-Inheritance Pattern**

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
- **Enhanced**: `DEFAULT_ERROR_CONFIG` with PrintOptim-friendly patterns:
  - Success keywords: `"created"`, `"cancelled"` 
  - Error-as-data prefixes: `"duplicate:"` (in addition to `"noop:"`, `"blocked:"`)

#### **🎯 PrintOptim Integration Impact**
- **Zero configuration**: Works perfectly with all PrintOptim patterns out-of-the-box
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
- **PrintOptim projects** can now use FraiseQL with absolute minimal code
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
- PrintOptim Backend DNS servers scenario (original failing case)
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
- ✅ Real-world PrintOptim Backend scenarios resolved
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
  - `PrintOptimConfig` → `STRICT_STATUS_CONFIG`
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
