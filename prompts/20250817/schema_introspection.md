# Schema Introspection Security Issue Fix - TDD Approach

## TDD Methodology - CRITICAL

**This issue MUST be solved using Test-Driven Development (TDD). Follow the RED → GREEN → REFACTOR cycle strictly.**

### Step 1: Investigate Existing Tests
**BEFORE writing any code, thoroughly search for existing introspection-related tests:**

```bash
# Search for introspection tests
rg -i "introspection" tests/
rg -i "__schema" tests/
rg -i "__type" tests/
grep -r "introspection" tests/
find tests/ -name "*introspection*"
find tests/ -name "*schema*" | grep -i test
```

**Check these specific test directories:**
- `tests/security/` - Security-related tests
- `tests/fastapi/` - FastAPI router tests
- `tests/gql/` - GraphQL schema tests
- `tests/core/` - Core functionality tests

**Look for existing test patterns around:**
- Schema querying
- Security configurations
- Environment-based behavior
- GraphQL endpoint testing

### Step 2: TDD Implementation Cycle

#### RED Phase - Write Failing Tests First
1. **Write security tests that fail** (introspection currently unprotected)
2. **Write configuration tests that fail** (no introspection config exists)
3. **Write environment tests that fail** (no environment-based control)

#### GREEN Phase - Minimal Implementation
1. **Implement only enough code to make tests pass**
2. **No over-engineering or premature optimization**
3. **Focus on the simplest solution that works**

#### REFACTOR Phase - Improve Design
1. **Clean up code while keeping tests green**
2. **Extract reusable components**
3. **Improve error handling and logging**

### Step 3: Test Categories to Implement

#### A. Existing Test Analysis
Document what introspection tests (if any) already exist and their current behavior.

#### B. Security Tests (Write FIRST)
```python
# Example failing test structure
async def test_introspection_blocked_in_production():
    """Introspection should be blocked in production mode."""
    # This test should FAIL initially
    pass

async def test_introspection_allowed_in_development():
    """Introspection should work in development mode."""
    # This test should FAIL initially
    pass
```

## Problem Statement

GraphQL schema introspection is currently unprotected in the FraiseQL framework, which poses a significant security risk. Introspection allows clients to query the schema structure and discover:

- Available queries, mutations, and subscriptions
- Field names and types
- Enum values
- Input types and their structure
- Documentation strings

This information exposure can aid attackers in understanding the API surface and identifying potential attack vectors.

## Security Requirements

1. **Production Protection**: Introspection should be disabled by default in production environments
2. **Development Flexibility**: Introspection should remain available in development/testing environments
3. **Configurable Control**: Provide configuration options to explicitly enable/disable introspection
4. **Authentication Awareness**: Consider user authentication/authorization when allowing introspection
5. **Audit Logging**: Log introspection attempts for security monitoring

## Implementation Areas

### 1. FastAPI Router Configuration
- Update `src/fraiseql/fastapi/routers.py` to handle introspection controls
- Add introspection middleware or filtering
- Respect environment-based configuration

### 2. GraphQL Schema Builder
- Modify `src/fraiseql/gql/schema_builder.py` to conditionally include introspection
- Ensure introspection fields are removed when disabled

### 3. Configuration Management
- Update `src/fraiseql/config/schema_config.py` or similar to include introspection settings
- Add environment variable support (e.g., `FRAISEQL_DISABLE_INTROSPECTION`)

### 4. Security Middleware
- Consider adding to `src/fraiseql/security/` module
- Implement introspection request detection and blocking

## Expected Behavior

### Production Mode
```python
# Introspection query should be rejected
query = """
{
  __schema {
    types {
      name
    }
  }
}
"""
# Should return error or empty result
```

### Development Mode
```python
# Introspection query should work normally
query = """
{
  __schema {
    types {
      name
      fields {
        name
        type {
          name
        }
      }
    }
  }
}
"""
# Should return full schema information
```

## TDD Test Implementation Strategy

### Phase 1: RED - Comprehensive Failing Tests

#### 1. Investigation Tests (Run First)
```python
def test_current_introspection_behavior():
    """Document current introspection behavior - baseline for TDD."""
    # Test what happens now with introspection queries
    # This helps understand the starting point
    pass

def test_existing_security_configurations():
    """Document existing security configurations."""
    # Check what security controls already exist
    pass
```

#### 2. Security Requirement Tests (Should Fail Initially)
```python
async def test_introspection_disabled_in_production():
    """RED: Introspection should be blocked in production mode."""
    # Set production environment
    # Send introspection query
    # Assert it's blocked
    assert False  # Will fail until implemented

async def test_introspection_enabled_in_development():
    """RED: Introspection should work in development mode."""
    # Set development environment
    # Send introspection query
    # Assert it works
    assert False  # Will fail until implemented

async def test_introspection_configurable():
    """RED: Introspection should be configurable via environment."""
    # Test FRAISEQL_DISABLE_INTROSPECTION=true
    # Test FRAISEQL_DISABLE_INTROSPECTION=false
    assert False  # Will fail until implemented
```

#### 3. Integration Tests (Should Fail Initially)
```python
async def test_fastapi_blocks_introspection_in_production():
    """RED: FastAPI endpoint should block introspection in production."""
    assert False  # Will fail until implemented

async def test_graphql_schema_excludes_introspection_fields():
    """RED: Schema should exclude introspection when disabled."""
    assert False  # Will fail until implemented
```

### Phase 2: GREEN - Minimal Implementation
1. **Run tests to confirm they fail**
2. **Implement minimal code to make each test pass**
3. **No additional features beyond what tests require**

### Phase 3: REFACTOR - Improve Design
1. **Extract configuration logic**
2. **Add proper error handling**
3. **Add logging and monitoring**
4. **Improve code organization**

### Testing Requirements by TDD Phase

#### RED Phase Tests
- [ ] Current behavior documentation tests
- [ ] Production introspection blocking tests
- [ ] Development introspection enabling tests
- [ ] Configuration-based control tests
- [ ] FastAPI integration tests
- [ ] GraphQL schema tests
- [ ] Error handling tests

#### GREEN Phase Implementation
- [ ] Minimal introspection control logic
- [ ] Basic environment detection
- [ ] Simple configuration system
- [ ] FastAPI integration
- [ ] Schema modification

#### REFACTOR Phase Improvements
- [ ] Clean configuration management
- [ ] Proper error messages
- [ ] Security logging
- [ ] Performance optimization
- [ ] Documentation updates

## Files to Examine/Modify

1. `src/fraiseql/fastapi/routers.py` - Main GraphQL endpoint
2. `src/fraiseql/gql/schema_builder.py` - Schema construction
3. `src/fraiseql/security/` - Security-related modules
4. `src/fraiseql/config/` - Configuration management
5. `tests/security/` - Security test suite

## TDD Success Criteria

### RED Phase Success
- [ ] **Existing introspection tests identified and documented**
- [ ] **All security requirement tests written and failing**
- [ ] **Test coverage for all scenarios identified**
- [ ] **Baseline behavior documented through tests**

### GREEN Phase Success
- [ ] **All RED phase tests now pass**
- [ ] **Introspection disabled by default in production**
- [ ] **Configurable via environment variables**
- [ ] **Proper error handling when introspection is blocked**
- [ ] **Minimal working implementation**

### REFACTOR Phase Success
- [ ] **Clean, maintainable code structure**
- [ ] **Comprehensive test coverage (>95%)**
- [ ] **Documentation updated**
- [ ] **Backward compatibility maintained for development use**
- [ ] **Security logging implemented**
- [ ] **Performance optimized**

## TDD Workflow Commands

```bash
# Step 1: Search for existing tests
rg -i "introspection" tests/
rg -i "__schema" tests/
rg -i "__type" tests/

# Step 2: Run tests to establish baseline
pytest tests/security/ -v
pytest tests/fastapi/ -v
pytest tests/gql/ -v

# Step 3: Create failing tests
# Create test file: tests/security/test_schema_introspection_security.py

# Step 4: TDD cycle
pytest tests/security/test_schema_introspection_security.py -v  # Should fail (RED)
# Implement minimal fix
pytest tests/security/test_schema_introspection_security.py -v  # Should pass (GREEN)
# Refactor and repeat
```

## Security Considerations

- Ensure the fix doesn't break legitimate development workflows
- Consider rate limiting for introspection queries when enabled
- Log introspection attempts for security monitoring
- Provide clear error messages that don't leak information about the restriction
