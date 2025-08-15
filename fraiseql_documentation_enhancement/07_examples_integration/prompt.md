# Prompt: Update Examples to Showcase New Patterns

## Objective

Update existing FraiseQL examples to showcase the newly documented PrintOptim patterns. This ensures that developers see these enterprise patterns in action and understand how to implement them in real applications.

## Current State

**Status: EXAMPLES NEED UPDATING**
- Existing examples use basic patterns
- Missing enterprise features like mutation results
- No NOOP handling demonstrations
- Limited audit trail examples

## Target Updates

Update existing example projects to demonstrate the new patterns while maintaining their educational value.

## Implementation Requirements

### 1. Update Blog API Example (`examples/blog_api/`)

**Files to update:**
- `mutations.py` - Add mutation result pattern
- `models.py` - Add audit trail types
- `db/functions/` - Add app/core function split
- `tests/` - Add validation testing

**Key changes:**

**`examples/blog_api/mutations.py`**
```python
"""Blog API mutations demonstrating PrintOptim patterns.

This example showcases enterprise-grade patterns:
- Mutation Result Pattern for standardized responses
- NOOP Handling for idempotent operations
- App/Core Function Split for clean architecture
- Comprehensive validation and error handling

For simpler patterns, see quickstart.py
"""

from fraiseql import mutation
from fraiseql.auth import requires_auth

# Import new pattern types
from models import (
    CreatePostSuccess, CreatePostError, CreatePostNoop,
    UpdatePostSuccess, UpdatePostError, UpdatePostNoop,
    AuditTrail
)

@mutation(function="app.create_post")  # Uses app/core split
class CreatePost:
    """Create blog post with enterprise patterns."""
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError
    noop: CreatePostNoop  # For duplicate handling

@mutation(function="app.update_post")
class UpdatePost:
    """Update blog post with change tracking."""
    input: UpdatePostInput
    success: UpdatePostSuccess
    error: UpdatePostError
    noop: UpdatePostNoop  # For no-changes scenarios

# Legacy resolvers updated to show pattern comparison
@requires_auth
async def create_post_legacy(info, input: CreatePostInput) -> Post:
    """Legacy pattern - for comparison only."""
    # Note: This shows the old way. Use CreatePost mutation class for new code.
    pass
```

**`examples/blog_api/models.py`**
```python
"""Blog API models demonstrating audit patterns."""

from datetime import datetime
from typing import Optional
from fraiseql import type, success, failure, input

@type
class AuditTrail:
    """Comprehensive audit information."""
    created_at: datetime
    created_by_name: Optional[str] = None
    updated_at: Optional[datetime] = None
    updated_by_name: Optional[str] = None
    version: int
    change_reason: Optional[str] = None
    updated_fields: Optional[list[str]] = None

@type
class Post:
    """Blog post with audit trail."""
    id: UUID
    title: str
    content: str
    is_published: bool

    # Enterprise features
    audit_trail: AuditTrail
    identifier: Optional[str] = None  # Business identifier

@success
class CreatePostSuccess:
    """Post created successfully."""
    post: Post
    message: str = "Post created successfully"
    was_noop: bool = False

@success
class CreatePostNoop:
    """Post creation was a no-op."""
    existing_post: Post
    message: str
    noop_reason: str
    was_noop: bool = True

@failure
class CreatePostError:
    """Post creation failed."""
    message: str
    error_code: str
    field_errors: Optional[dict[str, str]] = None
    validation_context: Optional[dict[str, Any]] = None

@input
class CreatePostInput:
    """Post creation input with validation."""
    title: Annotated[str, Field(min_length=3, max_length=200)]
    content: Annotated[str, Field(min_length=50)]
    is_published: bool = False

    # Audit metadata
    _change_reason: Optional[str] = None
    _expected_version: Optional[int] = None
```

**`examples/blog_api/db/functions/app_functions.sql`**
```sql
-- App layer functions (new file)
-- Demonstrates app/core function split pattern

-- App function: Input handling
CREATE OR REPLACE FUNCTION app.create_post(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_post_input;
BEGIN
    -- Convert JSONB to typed input (app layer responsibility)
    v_input := jsonb_populate_record(NULL::app.type_post_input, input_payload);

    -- Basic validation (app layer)
    IF v_input.title IS NULL OR length(trim(v_input.title)) < 3 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'post', NULL,
            'NOOP', 'noop:invalid_title', ARRAY[]::TEXT[],
            'Title must be at least 3 characters',
            NULL, NULL,
            jsonb_build_object('validation_layer', 'app')
        );
    END IF;

    -- Delegate to core layer
    RETURN core.create_post(input_pk_organization, input_created_by, v_input, input_payload);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- App function: Update with optimistic locking
CREATE OR REPLACE FUNCTION app.update_post(
    input_pk_organization UUID,
    input_updated_by UUID,
    input_pk_post UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_post_update_input;
    v_expected_version INTEGER;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_post_update_input, input_payload);
    v_expected_version := (input_payload->>'_expected_version')::INTEGER;

    RETURN core.update_post(
        input_pk_organization, input_updated_by, input_pk_post,
        v_input, v_expected_version, input_payload
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

**`examples/blog_api/db/functions/core_functions.sql`**
```sql
-- Core layer functions (new file)
-- Demonstrates business logic separation

CREATE OR REPLACE FUNCTION core.create_post(
    input_pk_organization UUID,
    input_created_by UUID,
    input_data app.type_post_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_post_id UUID;
    v_slug TEXT;
    v_duplicate_check RECORD;
BEGIN
    -- Business logic: Generate slug from title
    v_slug := core.generate_post_slug(input_data.title);

    -- Business logic: Check for duplicate slug (NOOP handling)
    SELECT pk_post, data INTO v_duplicate_check
    FROM tenant.tb_post
    WHERE fk_customer_org = input_pk_organization
    AND data->>'slug' = v_slug
    AND deleted_at IS NULL;

    -- NOOP: Post with same slug exists
    IF v_duplicate_check.pk_post IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'post', v_duplicate_check.pk_post,
            'NOOP', 'noop:slug_exists', ARRAY[]::TEXT[],
            'Post with similar title already exists',
            v_duplicate_check.data, v_duplicate_check.data,
            jsonb_build_object(
                'business_rule', 'unique_slug',
                'generated_slug', v_slug,
                'title_attempted', input_data.title
            )
        );
    END IF;

    -- Business logic: Auto-generate identifier
    DECLARE
        v_identifier TEXT;
    BEGIN
        v_identifier := core.generate_post_identifier(input_pk_organization, input_data.title);
    END;

    -- Create post with full audit trail
    INSERT INTO tenant.tb_post (
        pk_organization, data, created_by, created_at, updated_at, updated_by, version
    ) VALUES (
        input_pk_organization,
        jsonb_build_object(
            'title', input_data.title,
            'content', input_data.content,
            'slug', v_slug,
            'identifier', v_identifier,
            'is_published', COALESCE(input_data.is_published, false),
            'status', 'draft'
        ),
        input_created_by, NOW(), NOW(), input_created_by, 1
    ) RETURNING pk_post INTO v_post_id;

    -- Business logic: Create initial post stats
    PERFORM core.initialize_post_stats(v_post_id);

    -- Return with full audit information
    RETURN core.log_and_return_mutation(
        input_pk_organization, input_created_by, 'post', v_post_id,
        'INSERT', 'new',
        ARRAY['title', 'content', 'slug', 'identifier', 'is_published'],
        'Post created successfully',
        NULL,
        (SELECT data FROM public.v_post WHERE id = v_post_id),
        jsonb_build_object(
            'business_actions', ARRAY['slug_generated', 'identifier_assigned', 'stats_initialized'],
            'generated_slug', v_slug,
            'assigned_identifier', v_identifier
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### 2. Update E-commerce Example (`examples/ecommerce_api/`)

**Key additions:**
- Order processing with cross-entity validation
- Inventory management with business rules
- Payment processing with transaction patterns
- Comprehensive audit trails for financial data

**`examples/ecommerce_api/mutations.py`**
```python
"""E-commerce mutations demonstrating complex validation patterns."""

@mutation(function="app.process_order")
class ProcessOrder:
    """Process order with inventory validation."""
    input: ProcessOrderInput
    success: ProcessOrderSuccess
    error: ProcessOrderError
    noop: ProcessOrderNoop  # For inventory issues

@mutation(function="app.update_inventory")
class UpdateInventory:
    """Update inventory with business rules."""
    input: UpdateInventoryInput
    success: UpdateInventorySuccess
    error: UpdateInventoryError
    noop: UpdateInventoryNoop  # For no-change scenarios
```

### 3. Create New Advanced Example

**New directory: `examples/enterprise_patterns/`**

A comprehensive example showing all patterns together:

```
examples/enterprise_patterns/
├── README.md                    # Overview of all patterns
├── app.py                      # FastAPI app with all patterns
├── models.py                   # Complete type definitions
├── mutations.py                # All mutation patterns
├── queries.py                  # Advanced query patterns
├── db/
│   ├── migrations/
│   │   ├── 001_schema.sql     # Complete schema with all patterns
│   │   ├── 002_app_functions.sql   # App layer functions
│   │   ├── 003_core_functions.sql  # Core layer functions
│   │   └── 004_views.sql      # Views with audit trails
│   └── seeds/
│       └── sample_data.sql    # Test data
├── tests/
│   ├── test_mutation_results.py   # Mutation result pattern tests
│   ├── test_noop_handling.py      # NOOP pattern tests
│   ├── test_validation.py         # Multi-layer validation tests
│   ├── test_audit_trails.py       # Audit pattern tests
│   └── test_identifiers.py        # Identifier management tests
└── docker-compose.yml         # Complete setup
```

**`examples/enterprise_patterns/README.md`**
```markdown
# Enterprise Patterns Example

This example demonstrates all PrintOptim Backend patterns in a single, comprehensive application.

## Patterns Demonstrated

### ✅ Mutation Result Pattern
- Standardized mutation responses with metadata
- Field-level change tracking
- Comprehensive audit information
- See: `mutations.py` and `test_mutation_results.py`

### ✅ NOOP Handling Pattern
- Idempotent operations with graceful edge case handling
- Multiple NOOP scenarios (duplicate, no-changes, business rules)
- See: `test_noop_handling.py`

### ✅ App/Core Function Split
- Clean separation of input handling and business logic
- Type-safe core functions with JSONB app wrappers
- See: `db/migrations/002_app_functions.sql` and `003_core_functions.sql`

### ✅ Audit Field Patterns
- Complete audit trails with created/updated/deleted tracking
- Version management for optimistic locking
- Change reason and source tracking
- See: `models.py` (AuditTrail type) and audit field usage throughout

### ✅ Identifier Management
- Triple ID pattern: internal ID, UUID primary key, business identifier
- Automatic identifier generation and recalculation
- Flexible lookup by any identifier type
- See: identifier-related functions and tests

### ✅ Multi-Layer Validation
- GraphQL schema validation with Pydantic
- App layer input sanitization
- Core layer business rule validation
- Database constraint validation
- See: `test_validation.py`

## Quick Start

```bash
# Start database
docker-compose up -d db

# Run migrations
python -m examples.enterprise_patterns.migrations

# Start API
uvicorn examples.enterprise_patterns.app:app --reload

# Run tests
pytest examples/enterprise_patterns/tests/ -v
```

## Key Files

- **models.py** - Complete type definitions with all patterns
- **mutations.py** - All mutation patterns in one place
- **db/migrations/** - Complete schema demonstrating all patterns
- **tests/** - Comprehensive test suite for each pattern

This example serves as the definitive reference for implementing all patterns together.
```

### 4. Add Pattern Comparison Guide

**`examples/pattern_comparison.md`**
```markdown
# Pattern Comparison Guide

## Basic vs Enterprise Patterns

### Mutation Responses

**Basic Pattern (quickstart.py):**
```python
async def create_user(info, input: CreateUserInput) -> User:
    # Simple success/error handling
    user_id = await db.call_function("create_user", ...)
    return User.from_dict(result)
```

**Enterprise Pattern (blog_api/):**
```python
@mutation(function="app.create_user")
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess  # With metadata
    error: CreateUserError      # With context
    noop: CreateUserNoop       # For edge cases
```

### Function Architecture

**Basic Pattern:**
```sql
-- Single function with mixed concerns
CREATE FUNCTION create_user(input_data JSONB) RETURNS JSONB;
```

**Enterprise Pattern:**
```sql
-- App layer: Input handling
CREATE FUNCTION app.create_user(...) RETURNS app.mutation_result;

-- Core layer: Business logic
CREATE FUNCTION core.create_user(...) RETURNS app.mutation_result;
```

## When to Use Each Pattern

| Feature | Basic | Enterprise | Use When |
|---------|-------|-----------|----------|
| Simple CRUD | ✅ | ✅ | Learning, prototypes |
| Audit trails | ❌ | ✅ | Compliance required |
| NOOP handling | ❌ | ✅ | Idempotency needed |
| Complex validation | ❌ | ✅ | Business rules complex |
| Change tracking | ❌ | ✅ | Data governance required |

## Migration Path

1. Start with basic patterns for prototyping
2. Add mutation result pattern for better error handling
3. Implement audit fields for compliance
4. Add NOOP handling for reliability
5. Split functions for complex business logic
```

### 5. Update Documentation References

**Update all example README files to reference patterns:**

```markdown
<!-- Add to all example README files -->

## Patterns Used

This example demonstrates:
- ✅ Multi-tenancy with RLS
- ✅ CQRS with PostgreSQL functions
- ✅ [NEW] Mutation Result Pattern
- ✅ [NEW] NOOP Handling
- ❌ Advanced audit trails (see enterprise_patterns/ example)

For complete enterprise patterns, see `examples/enterprise_patterns/`.
```

## File Updates Summary

### New Files to Create:
- `examples/enterprise_patterns/` - Complete new example
- `examples/pattern_comparison.md` - Pattern comparison guide
- `examples/blog_api/db/functions/app_functions.sql`
- `examples/blog_api/db/functions/core_functions.sql`

### Files to Update:
- `examples/blog_api/mutations.py` - Add pattern comments and new classes
- `examples/blog_api/models.py` - Add enterprise types
- `examples/blog_api/README.md` - Reference new patterns
- `examples/ecommerce_api/mutations.py` - Add validation examples
- `examples/ecommerce_api/README.md` - Reference new patterns
- `examples/README.md` - Add enterprise_patterns example

### Comments to Add:
- All existing mutation functions get comments comparing to new patterns
- README files reference where to find enterprise versions
- Code comments explain when to use basic vs enterprise patterns

## Success Criteria

After implementation:
- [ ] All new patterns demonstrated in working examples
- [ ] Clear progression from basic to enterprise patterns
- [ ] Comprehensive test coverage for new patterns
- [ ] Documentation explains when to use each pattern
- [ ] Migration path from basic to enterprise clearly shown

## Implementation Methodology

### Development Workflow

**Critical: Incremental Example Integration**

Break this massive example update into manageable phases:

1. **Planning and Structure Commit** (15-20 minutes)
   ```bash
   # Plan all example updates and create structure
   git add examples/pattern_comparison.md examples/enterprise_patterns/README.md
   git commit -m "examples: plan enterprise pattern integration

   - Add pattern comparison guide structure
   - Create enterprise_patterns example outline
   - Document migration path from basic to enterprise
   - Plan file update strategy
   - References #[issue-number]"
   ```

2. **Blog API Enhancement Commit** (45-60 minutes)
   ```bash
   # Update blog API with new patterns
   git add examples/blog_api/
   git commit -m "examples: enhance blog API with enterprise patterns

   - Add app/core function split examples
   - Include mutation result pattern usage
   - Add NOOP handling demonstrations
   - Update README with pattern explanations"
   ```

3. **E-commerce Validation Commit** (35-45 minutes)
   ```bash
   # Add comprehensive validation to e-commerce
   git add examples/ecommerce_api/
   git commit -m "examples: add validation patterns to e-commerce

   - Implement multi-layer validation examples
   - Add cross-entity validation patterns
   - Include structured error responses
   - Document validation strategies"
   ```

4. **Enterprise Example Foundation Commit** (60-75 minutes)
   ```bash
   # Create comprehensive enterprise example
   git add examples/enterprise_patterns/
   git commit -m "examples: create enterprise patterns showcase

   - Implement complete enterprise example
   - Include all patterns in one cohesive system
   - Add comprehensive database schema
   - Create GraphQL API with all patterns"
   ```

5. **Testing and Documentation Commit** (40-50 minutes)
   ```bash
   # Complete test suites and documentation
   git add examples/enterprise_patterns/tests/ examples/*/tests/
   git commit -m "examples: add comprehensive test coverage

   - Test all new enterprise patterns
   - Include integration test scenarios
   - Add pattern-specific test examples
   - Document testing strategies"
   ```

6. **Integration and Polish Commit** (20-30 minutes)
   ```bash
   # Finalize with cross-references and documentation
   git add examples/README.md examples/*/README.md
   git commit -m "examples: complete enterprise pattern integration

   - Update all example README files
   - Add pattern usage matrices
   - Include cross-references between examples
   - Document when to use each pattern
   - Ready for review"
   ```

### Quality Validation

After each commit:
- [ ] All examples run without errors
- [ ] Test suites pass completely
- [ ] Documentation builds correctly
- [ ] SQL syntax validates
- [ ] GraphQL schemas are valid
- [ ] Examples demonstrate patterns correctly

### Risk Management

**For large example creation:**
```bash
# Test examples incrementally
# Start API servers to validate functionality
# Run test suites after each major change
# Keep examples simple but comprehensive
```

**For backward compatibility:**
```bash
# Don't break existing examples
# Add new patterns alongside existing code
# Use comments to explain differences
# Maintain existing API contracts
```

**Recovery strategy:**
```bash
# Large example updates are high-risk
git branch example-backup  # Save current state
# Work on feature branches for complex changes
# Test each example independently
```

### Development Environment

**Before starting:**
```bash
# Ensure all pattern documentation is complete
# Set up development database
# Verify existing examples work
# Plan file structure changes
```

**During development:**
```bash
# Test each example as you build it
python -m pytest examples/enterprise_patterns/tests/ -v
uvicorn examples.enterprise_patterns.app:app --reload
# Validate against pattern documentation
```

## Dependencies

Should be done after:
- All pattern documentation is complete
- New patterns are fully specified and tested
- Examples maintain backward compatibility

## Estimated Effort

**Large effort** - Comprehensive example updates:
- Complete new enterprise example
- Multiple file updates across existing examples
- Comprehensive test suite additions
- Pattern comparison documentation

Target: 2000+ lines of new example code and tests
