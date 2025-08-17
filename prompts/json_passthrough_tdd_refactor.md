# JSON Passthrough TDD Refactor Prompt

## Context
The JSON passthrough feature has accumulated complexity over multiple fixes and patches. The code is spread across many files with overlapping responsibilities and unclear boundaries. Recent commits show attempts to fix configuration issues but the overall design needs simplification.

## Goal
Create a clean, simple JSON passthrough implementation using strict TDD methodology. Start from scratch with tests that define the desired behavior, then implement only what's necessary to make tests pass.

## Core Requirements

### 1. What is JSON Passthrough?
JSON passthrough is a performance optimization that bypasses Python object instantiation when serving GraphQL responses. Instead of:
- Database → Python objects → GraphQL type validation → JSON response

We do:
- Database → Direct JSON response (with minimal wrapper for field resolution)

### 2. When Should it Activate?
The system should use passthrough when:
- Environment is "production" or "staging" (configurable)
- Config explicitly enables it: `json_passthrough_enabled = True`
- Config allows it in production: `json_passthrough_in_production = True`
- OR request header explicitly requests it: `X-JSON-Passthrough: true`

### 3. Key Design Principles
- **Single Responsibility**: Each component has ONE clear job
- **Explicit over Implicit**: No magic, clear activation paths
- **Testable**: Every behavior can be unit tested in isolation
- **Minimal Surface Area**: Fewer files, fewer classes, less complexity

## TDD Test Structure

### Phase 1: Core Wrapper Tests
```python
# tests/passthrough/test_json_wrapper.py

def test_wrapper_provides_attribute_access():
    """Wrapper should allow dot notation access to dict keys."""

def test_wrapper_handles_nested_objects():
    """Nested dicts should be wrapped automatically."""

def test_wrapper_handles_lists():
    """Lists of dicts should be wrapped, scalar lists returned as-is."""

def test_wrapper_provides_typename():
    """__typename should be injected for GraphQL compatibility."""

def test_wrapper_supports_camelcase_conversion():
    """snake_case fields should be accessible as camelCase."""
```

### Phase 2: Configuration Tests
```python
# tests/passthrough/test_config.py

def test_passthrough_disabled_by_default():
    """Passthrough should be opt-in, not default."""

def test_passthrough_enabled_via_config():
    """Config flag should enable passthrough in production."""

def test_passthrough_disabled_in_development():
    """Development mode should never use passthrough (for debugging)."""

def test_passthrough_forced_via_header():
    """X-JSON-Passthrough header should override config."""
```

### Phase 3: Repository Integration Tests
```python
# tests/passthrough/test_repository.py

def test_repository_returns_wrapped_objects_when_enabled():
    """Repository should return wrapped dicts when passthrough is on."""

def test_repository_returns_objects_when_disabled():
    """Repository should return normal objects when passthrough is off."""

def test_repository_respects_context_flags():
    """Repository should check context for passthrough decision."""
```

### Phase 4: Router Integration Tests
```python
# tests/passthrough/test_router.py

def test_router_sets_passthrough_context():
    """Router should set passthrough flags in context."""

def test_router_returns_raw_json_response():
    """Router should return RawJSONResponse when appropriate."""

def test_router_respects_environment_config():
    """Router should honor environment-based configuration."""
```

### Phase 5: End-to-End Tests
```python
# tests/passthrough/test_e2e.py

def test_graphql_query_with_passthrough():
    """Full query should work with passthrough enabled."""

def test_graphql_query_without_passthrough():
    """Full query should work with passthrough disabled."""

def test_performance_improvement():
    """Passthrough should be measurably faster than object instantiation."""
```

## Implementation Plan

### Step 1: Create Clean Test Suite
1. Create `tests/passthrough/` directory
2. Write all test files with failing tests
3. Tests define the complete API surface

### Step 2: Implement Core Wrapper
1. Create `src/fraiseql/passthrough/wrapper.py`
   - Simple class that wraps dict
   - Provides __getattr__ for field access
   - Handles nesting and lists
   - NO dependencies on other FraiseQL code

### Step 3: Implement Configuration
1. Create `src/fraiseql/passthrough/config.py`
   - Simple config class
   - Clear rules for when passthrough activates
   - NO complex inheritance or mixins

### Step 4: Integrate with Repository
1. Create `src/fraiseql/passthrough/repository.py`
   - Simple wrapper/decorator for repository methods
   - Checks context and wraps results
   - NO modification of existing repository code

### Step 5: Integrate with Router
1. Update router to set context flags
2. Update router to handle wrapped responses
3. Minimal changes to existing code

## Files to Delete/Consolidate

After the refactor, these files should be removed or consolidated:
- `src/fraiseql/core/json_passthrough.py` → Replace with simpler wrapper
- `src/fraiseql/repositories/passthrough_mixin.py` → Replace with decorator
- `src/fraiseql/gql/raw_json_wrapper.py` → Consolidate
- `src/fraiseql/gql/raw_json_resolver.py` → Consolidate
- `src/fraiseql/gql/json_executor.py` → Consolidate
- `src/fraiseql/core/json_passthrough_repository.py` → Remove
- `src/fraiseql/graphql/passthrough_context.py` → Simplify

## Success Criteria

1. **All tests pass** - Every test written in TDD phase passes
2. **Fewer files** - Reduce from 10+ files to 3-4 files
3. **Clear boundaries** - Each component has clear responsibility
4. **No regressions** - Existing functionality continues to work
5. **Performance** - Measurable improvement in production mode
6. **Simplicity** - Code is obvious, no hidden complexity

## Testing Commands

```bash
# Run only passthrough tests during development
pytest tests/passthrough/ -xvs

# Run with coverage to ensure complete testing
pytest tests/passthrough/ --cov=src/fraiseql/passthrough --cov-report=term-missing

# Run performance comparison
pytest tests/passthrough/test_e2e.py::test_performance_improvement -xvs

# Run all tests to ensure no regressions
pytest tests/
```

## Key Decisions to Make

1. **Wrapper Design**: Should we use __getattr__ or generate properties?
2. **Activation Logic**: Should context or config take precedence?
3. **Response Format**: Raw JSON string or structured dict?
4. **Error Handling**: What happens when passthrough fails?
5. **Backward Compatibility**: How to migrate existing code?

## Notes

- Start with the simplest possible implementation
- Add complexity only when tests require it
- Every line of code should exist to make a test pass
- If a test doesn't require it, don't implement it
- Keep the "production path" as lean as possible
