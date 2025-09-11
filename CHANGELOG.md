# Changelog

All notable changes to FraiseQL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
