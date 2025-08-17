# Prompt: Create Migration Guides for New Patterns

## Objective

Create comprehensive migration guides to help existing FraiseQL users adopt the new PrintOptim patterns. These guides should provide step-by-step instructions, code comparisons, and migration strategies that minimize disruption to existing applications.

## Current State

**Status: NO MIGRATION GUIDANCE**
- No documentation for migrating existing FraiseQL applications
- Users may not know how to adopt new patterns
- Risk of breaking changes without proper guidance
- Missing incremental adoption strategies

## Target Documentation

Create new documentation file: `docs/migration/printoptim-patterns-migration.md`

## Implementation Requirements

### 1. Document Overall Migration Strategy

**Incremental adoption approach:**
```markdown
# Migration Strategy: Basic → Enterprise Patterns

## Phase 1: Foundation (Week 1-2)
✅ **Safe to implement immediately:**
1. Add audit fields to existing tables
2. Implement mutation result pattern for new mutations
3. Add app/core split for new functions

❌ **Avoid breaking changes:**
- Don't modify existing function signatures
- Don't change existing GraphQL types immediately
- Keep existing resolvers working

## Phase 2: Enhancement (Week 3-4)
✅ **Enhance existing functionality:**
4. Add NOOP handling to critical operations
5. Implement identifier management for new entities
6. Add validation patterns to new inputs

## Phase 3: Optimization (Month 2)
✅ **Optimize and standardize:**
7. Migrate existing functions to app/core split
8. Add comprehensive validation to existing operations
9. Implement advanced audit features

## Phase 4: Completion (Month 3)
✅ **Full enterprise patterns:**
10. Complete NOOP handling across all operations
11. Advanced caching and performance optimization
12. Complete audit compliance features
```

### 2. Document Audit Field Migration

**Adding audit fields to existing tables:**
```sql
-- Step 1: Add audit columns (non-breaking)
ALTER TABLE tenant.tb_user
ADD COLUMN created_at TIMESTAMPTZ,
ADD COLUMN created_by UUID,
ADD COLUMN updated_at TIMESTAMPTZ,
ADD COLUMN updated_by UUID,
ADD COLUMN version INTEGER DEFAULT 1,
ADD COLUMN change_reason TEXT,
ADD COLUMN change_source TEXT DEFAULT 'migration';

-- Step 2: Backfill audit data for existing records
UPDATE tenant.tb_user
SET
    created_at = COALESCE(created_at, '2024-01-01'::TIMESTAMPTZ),
    updated_at = COALESCE(updated_at, '2024-01-01'::TIMESTAMPTZ),
    created_by = COALESCE(created_by, '00000000-0000-0000-0000-000000000000'::UUID),
    change_reason = 'Backfilled during migration',
    change_source = 'migration'
WHERE created_at IS NULL;

-- Step 3: Make required fields NOT NULL (after backfill)
ALTER TABLE tenant.tb_user
ALTER COLUMN created_at SET NOT NULL,
ALTER COLUMN updated_at SET NOT NULL,
ALTER COLUMN created_by SET NOT NULL;

-- Step 4: Add automatic updated_at trigger (optional)
CREATE OR REPLACE FUNCTION trigger_update_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_user_update_timestamp
    BEFORE UPDATE ON tenant.tb_user
    FOR EACH ROW
    EXECUTE FUNCTION trigger_update_timestamp();
```

### 3. Document Function Migration

**Converting existing functions to app/core split:**

**Before (existing monolithic function):**
```sql
CREATE OR REPLACE FUNCTION fn_create_user(input_data JSONB)
RETURNS JSONB AS $$
DECLARE
    v_user_id UUID;
    v_email TEXT;
    v_name TEXT;
BEGIN
    -- Mixed input parsing and business logic
    v_email := input_data->>'email';
    v_name := input_data->>'name';

    IF v_email IS NULL THEN
        RETURN jsonb_build_object('success', false, 'error', 'Email required');
    END IF;

    IF EXISTS (SELECT 1 FROM tenant.tb_user WHERE data->>'email' = v_email) THEN
        RETURN jsonb_build_object('success', false, 'error', 'Email exists');
    END IF;

    INSERT INTO tenant.tb_user (data, created_at)
    VALUES (jsonb_build_object('email', v_email, 'name', v_name), NOW())
    RETURNING pk_user INTO v_user_id;

    RETURN jsonb_build_object('success', true, 'user_id', v_user_id);
END;
$$ LANGUAGE plpgsql;
```

**After (app/core split with backward compatibility):**
```sql
-- Step 1: Create input type
CREATE TYPE app.type_user_input AS (
    email TEXT,
    name TEXT,
    bio TEXT
);

-- Step 2: Create core function (business logic)
CREATE OR REPLACE FUNCTION core.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_data app.type_user_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_user_id UUID;
    v_existing_user RECORD;
BEGIN
    -- Business logic: Check for existing user
    SELECT pk_user, data INTO v_existing_user
    FROM tenant.tb_user
    WHERE pk_organization = input_pk_organization
    AND data->>'email' = input_data.email;

    -- NOOP: User already exists
    IF v_existing_user.pk_user IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'user', v_existing_user.pk_user,
            'NOOP', 'noop:already_exists', ARRAY[]::TEXT[],
            'User with this email already exists',
            v_existing_user.data, v_existing_user.data,
            jsonb_build_object('business_rule', 'unique_email')
        );
    END IF;

    -- Create user with audit fields
    INSERT INTO tenant.tb_user (
        pk_organization, data,
        created_at, created_by, updated_at, updated_by, version
    ) VALUES (
        input_pk_organization,
        jsonb_build_object('email', input_data.email, 'name', input_data.name, 'bio', input_data.bio),
        NOW(), input_created_by, NOW(), input_created_by, 1
    ) RETURNING pk_user INTO v_user_id;

    RETURN core.log_and_return_mutation(
        input_pk_organization, input_created_by, 'user', v_user_id,
        'INSERT', 'new', ARRAY['email', 'name', 'bio'],
        'User created successfully',
        NULL, (SELECT data FROM public.v_user WHERE id = v_user_id),
        jsonb_build_object('migration_source', 'converted_function')
    );
END;
$$ LANGUAGE plpgsql;

-- Step 3: Create app function (input handling)
CREATE OR REPLACE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_user_input;
BEGIN
    -- Parse JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);

    -- Basic validation (app layer)
    IF v_input.email IS NULL OR v_input.name IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'user', NULL,
            'NOOP', 'noop:invalid_input', ARRAY[]::TEXT[],
            'Email and name are required',
            NULL, NULL,
            jsonb_build_object('validation_layer', 'app')
        );
    END IF;

    -- Delegate to core
    RETURN core.create_user(input_pk_organization, input_created_by, v_input, input_payload);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Step 4: Update legacy function to use new architecture (backward compatibility)
CREATE OR REPLACE FUNCTION fn_create_user(input_data JSONB)
RETURNS JSONB AS $$
DECLARE
    v_result app.mutation_result;
    v_legacy_result JSONB;
BEGIN
    -- Call new app function
    SELECT * INTO v_result FROM app.create_user(
        '11111111-1111-1111-1111-111111111111'::UUID, -- Default org for migration
        '00000000-0000-0000-0000-000000000000'::UUID, -- Default user for migration
        input_data
    );

    -- Convert new result format back to legacy format
    IF v_result.status = 'new' THEN
        v_legacy_result := jsonb_build_object(
            'success', true,
            'user_id', v_result.id,
            'user', v_result.object_data
        );
    ELSIF v_result.status LIKE 'noop:%' THEN
        v_legacy_result := jsonb_build_object(
            'success', false,
            'error', v_result.message,
            'existing_user', v_result.object_data
        );
    ELSE
        v_legacy_result := jsonb_build_object(
            'success', false,
            'error', v_result.message
        );
    END IF;

    RETURN v_legacy_result;
END;
$$ LANGUAGE plpgsql;
```

### 4. Document GraphQL Resolver Migration

**Upgrading resolvers to use mutation result pattern:**

**Before (basic resolver):**
```python
@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    """Basic mutation resolver."""
    db = info.context["db"]

    result = await db.call_function("fn_create_user", input.to_dict())

    if not result.get("success"):
        raise GraphQLError(result.get("error", "Creation failed"))

    user_data = await db.find_one("v_user", where={"id": result["user_id"]})
    return User.from_dict(user_data)
```

**After (mutation result pattern with backward compatibility):**
```python
# New pattern - recommended for new code
@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"
    metadata: dict[str, Any] | None = None

@fraiseql.success
class CreateUserNoop:
    existing_user: User
    message: str
    noop_reason: str

@fraiseql.failure
class CreateUserError:
    message: str
    error_code: str

# New mutation class - preferred approach
@fraiseql.mutation(function="app.create_user")
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    noop: CreateUserNoop
    error: CreateUserError

# Legacy resolver - updated to use new backend but keep interface
@fraiseql.mutation
async def create_user_legacy(info, input: CreateUserInput) -> User:
    """Legacy resolver - maintained for backward compatibility."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]
    user_id = info.context["user_id"]

    # Call new app function
    result = await db.call_function(
        "app.create_user",
        input_pk_organization=tenant_id,
        input_created_by=user_id,
        input_payload=input.to_dict()
    )

    # Convert mutation result to legacy response
    if result["status"] == "new":
        return User.from_dict(result["object_data"])
    elif result["status"].startswith("noop:"):
        if result["status"] == "noop:already_exists":
            # For legacy compatibility, return existing user instead of error
            return User.from_dict(result["object_data"])
        else:
            raise GraphQLError(result["message"])
    else:
        raise GraphQLError(result.get("message", "User creation failed"))
```

### 5. Document View Migration

**Adding audit fields to existing views:**

**Before:**
```sql
CREATE VIEW v_user AS
SELECT
    pk_user::TEXT AS id,
    data->>'email' AS email,
    data->>'name' AS name
FROM tenant.tb_user;
```

**After (with audit fields):**
```sql
CREATE OR REPLACE VIEW v_user AS
SELECT
    u.pk_user::TEXT AS id,
    u.data->>'email' AS email,
    u.data->>'name' AS name,
    u.data->>'bio' AS bio,

    -- Audit fields
    u.created_at,
    cu.data->>'name' AS created_by_name,
    u.updated_at,
    uu.data->>'name' AS updated_by_name,
    u.version,
    u.change_reason,

    -- Audit trail object for GraphQL
    jsonb_build_object(
        'created_at', u.created_at,
        'created_by', u.created_by,
        'created_by_name', cu.data->>'name',
        'updated_at', u.updated_at,
        'updated_by', u.updated_by,
        'updated_by_name', uu.data->>'name',
        'version', u.version,
        'change_reason', u.change_reason
    ) AS audit_trail,

    -- Complete data object
    jsonb_build_object(
        'id', u.pk_user,
        'email', u.data->>'email',
        'name', u.data->>'name',
        'bio', u.data->>'bio',
        'audit_trail', jsonb_build_object(
            'created_at', u.created_at,
            'version', u.version
        )
    ) AS data

FROM tenant.tb_user u
LEFT JOIN tenant.tb_user cu ON cu.pk_user = u.created_by
LEFT JOIN tenant.tb_user uu ON uu.pk_user = u.updated_by
WHERE u.deleted_at IS NULL;
```

### 6. Document Incremental Testing Strategy

**Testing during migration:**
```python
"""Migration testing strategy."""

import pytest
from typing import Union

class TestMigrationCompatibility:
    """Test that new patterns maintain backward compatibility."""

    async def test_legacy_function_still_works(self, db):
        """Ensure old function calls still work."""
        # Test legacy function interface
        result = await db.call_function(
            "fn_create_user",
            {"email": "test@example.com", "name": "Test User"}
        )

        assert result["success"] is True
        assert "user_id" in result

    async def test_new_function_provides_more_data(self, db):
        """Verify new function provides enhanced data."""
        result = await db.call_function(
            "app.create_user",
            input_pk_organization=TEST_TENANT_ID,
            input_created_by=TEST_USER_ID,
            input_payload={"email": "new@example.com", "name": "New User"}
        )

        # New format includes audit trail
        assert result["status"] == "new"
        assert result["updated_fields"] is not None
        assert result["extra_metadata"] is not None

    async def test_legacy_resolver_compatibility(self, graphql_client):
        """Test legacy GraphQL resolvers work with new backend."""
        # Legacy mutation should still work
        result = await graphql_client.execute("""
            mutation {
                createUserLegacy(input: {
                    email: "legacy@example.com"
                    name: "Legacy User"
                }) {
                    id
                    email
                    name
                }
            }
        """)

        assert "errors" not in result
        assert result["data"]["createUserLegacy"]["email"] == "legacy@example.com"

    async def test_new_mutation_class(self, graphql_client):
        """Test new mutation class with result pattern."""
        result = await graphql_client.execute("""
            mutation {
                createUser(input: {
                    email: "new@example.com"
                    name: "New User"
                }) {
                    __typename
                    ... on CreateUserSuccess {
                        user {
                            id
                            email
                            auditTrail {
                                createdAt
                                version
                            }
                        }
                        message
                    }
                    ... on CreateUserNoop {
                        existingUser { id }
                        message
                        noopReason
                    }
                }
            }
        """)

        assert result["data"]["createUser"]["__typename"] == "CreateUserSuccess"
        assert result["data"]["createUser"]["user"]["auditTrail"]["version"] == 1
```

### 7. Document Performance Impact

**Migration performance considerations:**
```sql
-- Migration performance tips

-- 1. Add indexes before backfilling data
CREATE INDEX CONCURRENTLY idx_user_created_at ON tenant.tb_user(created_at)
WHERE created_at IS NOT NULL;

-- 2. Batch large updates to avoid lock contention
DO $$
DECLARE
    v_batch_size INTEGER := 1000;
    v_processed INTEGER := 0;
BEGIN
    LOOP
        WITH batch AS (
            SELECT pk_user
            FROM tenant.tb_user
            WHERE created_at IS NULL
            LIMIT v_batch_size
        )
        UPDATE tenant.tb_user
        SET created_at = '2024-01-01'::TIMESTAMPTZ,
            updated_at = '2024-01-01'::TIMESTAMPTZ
        FROM batch
        WHERE tenant.tb_user.pk_user = batch.pk_user;

        GET DIAGNOSTICS v_processed = ROW_COUNT;
        EXIT WHEN v_processed = 0;

        -- Brief pause to avoid overwhelming the system
        PERFORM pg_sleep(0.1);
    END LOOP;
END $$;

-- 3. Analyze tables after migration
ANALYZE tenant.tb_user;
```

### 8. Document Common Migration Issues

**Troubleshooting guide:**
```markdown
## Common Migration Issues

### Issue: "column does not exist" after adding audit fields
**Cause**: Views not updated to handle new columns
**Solution**: Recreate views with `CREATE OR REPLACE VIEW`

### Issue: Legacy functions break after migration
**Cause**: Function signature changes
**Solution**: Maintain wrapper functions for backward compatibility

### Issue: Performance degradation after audit field addition
**Cause**: Missing indexes on new audit columns
**Solution**: Add indexes on `created_at`, `updated_at`, and `version`

### Issue: GraphQL schema errors with new types
**Cause**: Frontend not updated to handle new union types
**Solution**: Use incremental rollout - keep legacy resolvers during transition

### Issue: NOOP responses break existing error handling
**Cause**: Frontend expects errors for duplicates, not successful NOOPs
**Solution**: Configure NOOP behavior per client using request headers or flags
```

### 9. Documentation Structure

Create comprehensive migration guide:
1. **Migration Overview** - Strategy and phases
2. **Database Migration** - Schema and data migration steps
3. **Function Migration** - Converting to app/core split
4. **GraphQL Migration** - Updating resolvers and types
5. **View Migration** - Adding audit field support
6. **Testing Strategy** - Maintaining compatibility during migration
7. **Performance Considerations** - Optimizing migration process
8. **Rollback Plan** - How to revert changes if needed
9. **Troubleshooting** - Common issues and solutions
10. **Migration Checklist** - Step-by-step validation

## Success Criteria

After implementation:
- [ ] Complete step-by-step migration guide
- [ ] Backward compatibility strategies documented
- [ ] Testing approach for migration validation
- [ ] Performance optimization guidance included
- [ ] Troubleshooting guide for common issues
- [ ] Rollback procedures documented
- [ ] Follows FraiseQL documentation style

## File Location

Create: `docs/migration/printoptim-patterns-migration.md`

Create: `docs/migration/index.md` (if doesn't exist)

Update: `docs/index.md` to include migration section

## Implementation Methodology

### Development Workflow

**Critical: Practical Migration Documentation**

Break this comprehensive migration guide into practical phases:

1. **Migration Strategy Foundation Commit** (20-25 minutes)
   ```bash
   # Establish migration philosophy and overview
   git add docs/migration/printoptim-patterns-migration.md docs/migration/index.md
   git commit -m "docs: initialize PrintOptim patterns migration guide

   - Define phased migration strategy
   - Document backward compatibility approach
   - Create migration phases overview
   - Add risk assessment framework
   - References #[issue-number]"
   ```

2. **Database Migration Patterns Commit** (35-45 minutes)
   ```bash
   # Complete database schema migration steps
   git add docs/migration/printoptim-patterns-migration.md
   git commit -m "docs: add database migration patterns

   - Document audit field addition process
   - Show table restructuring for triple ID pattern
   - Include data backfilling strategies
   - Add index creation for performance"
   ```

3. **Function Migration Patterns Commit** (40-50 minutes)
   ```bash
   # Complete function refactoring guidance
   git add docs/migration/printoptim-patterns-migration.md
   git commit -m "docs: add function migration patterns

   - Document app/core function split process
   - Show legacy function preservation strategies
   - Include mutation result pattern adoption
   - Add NOOP handling integration steps"
   ```

4. **GraphQL Migration Patterns Commit** (30-40 minutes)
   ```bash
   # Complete GraphQL resolver and type migrations
   git add docs/migration/printoptim-patterns-migration.md
   git commit -m "docs: add GraphQL migration patterns

   - Document resolver refactoring steps
   - Show type system updates for new patterns
   - Include client compatibility strategies
   - Add incremental rollout approaches"
   ```

5. **Testing and Validation Commit** (35-45 minutes)
   ```bash
   # Complete testing strategy and validation
   git add docs/migration/printoptim-patterns-migration.md
   git commit -m "docs: add migration testing and validation

   - Document compatibility testing strategies
   - Include performance validation approaches
   - Add data integrity verification steps
   - Show rollback testing procedures"
   ```

6. **Troubleshooting and Finalization Commit** (25-30 minutes)
   ```bash
   # Complete with troubleshooting and integration
   git add docs/migration/printoptim-patterns-migration.md docs/index.md
   git commit -m "docs: complete PrintOptim migration guide

   - Add comprehensive troubleshooting section
   - Include performance optimization guidance
   - Document common pitfalls and solutions
   - Update main documentation index
   - Ready for review"
   ```

### Quality Validation

After each commit:
- [ ] Build documentation (`mkdocs serve`)
- [ ] Validate all SQL migration examples
- [ ] Test migration scripts in development environment
- [ ] Verify backward compatibility preservation
- [ ] Check rollback procedures work correctly
- [ ] Ensure migration steps are practical and actionable

### Risk Management

**For migration SQL examples:**
```bash
# Test all migration scripts in development first
# Validate backup and restore procedures
# Include timing estimates for large tables
# Document rollback for each migration step
```

**For backward compatibility:**
```bash
# Test legacy API calls still work
# Verify existing GraphQL queries unchanged
# Include gradual adoption strategies
# Document feature flags for controlled rollout
```

**Recovery strategy:**
```bash
# Migration guides are critical - test thoroughly
git branch migration-backup  # Save working version
# Test migration steps in separate database
# Validate each step before documenting
```

### Real-World Testing

**Before finalizing:**
```bash
# Create test database with legacy schema
# Run migration steps exactly as documented
# Verify each phase preserves functionality
# Test rollback procedures work correctly
# Include performance impact measurements
```

## Dependencies

Should be created after:
- All pattern documentation is complete
- Examples are updated with new patterns
- Testing strategies are validated

## Estimated Effort

**Large effort** - Comprehensive migration guidance:
- Complete migration strategy documentation
- Step-by-step instructions for each pattern
- Backward compatibility preservation
- Testing and troubleshooting guidance

Target: 1500-2000 lines of migration documentation
