# Prompt: Implement App/Core Function Split Pattern Documentation

## Objective

Create comprehensive documentation for FraiseQL's app/core function split pattern, which is the **foundation** of enterprise-grade PostgreSQL function architecture. This pattern separates concerns between input handling (app) and business logic (core), enabling better testing, modularity, and maintainability.

## Current State

**Status: PARTIALLY DOCUMENTED (25% coverage)**
- FraiseQL documents basic PostgreSQL functions
- Missing the critical app/core architectural split
- No documentation of the two-layer function pattern
- Examples show monolithic function approach

## Target Documentation

Create new documentation section in: `docs/mutations/postgresql-function-based.md`
Add new section: **"Enterprise Function Architecture: App/Core Split Pattern"**

## Implementation Requirements

### 1. Document Core Architecture Philosophy

**Two-layer function architecture:**
```
GraphQL → app.* functions → core.* functions → Database
          ↓                 ↓
          JSONB input       Typed input
          Thin wrapper      Business logic
          Type conversion   Validation & processing
```

**Separation of concerns:**
- **app.* schema**: Thin wrapper functions handling JSONB input from GraphQL
- **core.* schema**: Pure business logic with typed parameters
- **Clear boundaries**: No business logic in app, no JSONB handling in core

### 2. Document App Schema Functions

**Purpose of app.* functions:**
- Accept JSONB input from GraphQL resolvers
- Convert JSONB to typed PostgreSQL records
- Call corresponding core.* function
- Return app.mutation_result

**Standard app function pattern:**
```sql
-- App layer: Thin wrapper for JSONB input
CREATE OR REPLACE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_user_input;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);

    -- Validate required fields (minimal validation only)
    IF v_input.email IS NULL OR v_input.name IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            NULL,
            'NOOP',
            'noop:invalid_input',
            ARRAY[]::TEXT[],
            'Email and name are required',
            NULL,
            NULL,
            jsonb_build_object('trigger', 'api_create', 'validation_layer', 'app')
        );
    END IF;

    -- Delegate to core function
    RETURN core.create_user(
        input_pk_organization,
        input_created_by,
        v_input,
        input_payload  -- Pass original for logging
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

### 3. Document Core Schema Functions

**Purpose of core.* functions:**
- Contain ALL business logic
- Work with typed parameters (not JSONB)
- Perform complex validation
- Handle transaction logic
- Call other core functions
- Return via core.log_and_return_mutation

**Standard core function pattern:**
```sql
-- Core layer: All business logic
CREATE OR REPLACE FUNCTION core.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_data app.type_user_input,
    input_payload JSONB  -- For logging only
) RETURNS app.mutation_result AS $$
DECLARE
    v_user_id UUID;
    v_existing_user RECORD;
    v_payload_after JSONB;
BEGIN
    -- Business logic: Check for duplicates
    SELECT pk_user, data INTO v_existing_user
    FROM tenant.tb_user
    WHERE pk_organization = input_pk_organization
    AND data->>'email' = input_data.email;

    -- Business rule: Handle duplicate
    IF v_existing_user.pk_user IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            v_existing_user.pk_user,
            'NOOP',
            'noop:already_exists',
            ARRAY[]::TEXT[],
            'User with this email already exists',
            v_existing_user.data,
            v_existing_user.data,
            jsonb_build_object(
                'trigger', 'api_create',
                'business_rule', 'unique_email',
                'existing_email', input_data.email
            )
        );
    END IF;

    -- Business logic: Create user with role assignment
    INSERT INTO tenant.tb_user (pk_organization, data, created_by)
    VALUES (
        input_pk_organization,
        jsonb_build_object(
            'email', input_data.email,
            'name', input_data.name,
            'role', core.assign_default_user_role(input_data.email, input_pk_organization),
            'profile_status', 'pending_verification'
        ),
        input_created_by
    ) RETURNING pk_user INTO v_user_id;

    -- Business logic: Send welcome email
    PERFORM core.queue_welcome_email(v_user_id, input_data.email);

    -- Business logic: Update organization stats
    PERFORM core.increment_organization_user_count(input_pk_organization);

    -- Get complete user data from view
    SELECT data INTO v_payload_after
    FROM public.tv_user
    WHERE id = v_user_id;

    -- Return success via centralized logging
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_created_by,
        'user',
        v_user_id,
        'INSERT',
        'new',
        ARRAY['email', 'name', 'role', 'profile_status'],
        'User created successfully',
        NULL,
        v_payload_after,
        jsonb_build_object(
            'trigger', 'api_create',
            'business_actions', ARRAY[
                'role_assigned',
                'welcome_email_queued',
                'org_stats_updated'
            ]
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### 4. Document Input Type Definitions

**App schema type definitions:**
```sql
-- Input type for app functions
CREATE TYPE app.type_user_input AS (
    email TEXT,
    name TEXT,
    bio TEXT,
    avatar_url TEXT,
    role TEXT
);

-- Complex input with nested data
CREATE TYPE app.type_contract_input AS (
    identifier TEXT,
    name TEXT,
    start_date DATE,
    end_date DATE,
    terms JSONB,
    items JSONB[]  -- Array of contract items
);
```

### 5. Document Benefits of Split Pattern

**Advantages:**
1. **Testability**: Core functions can be tested with typed parameters
2. **Reusability**: Core functions can be called by different app functions
3. **Maintainability**: Business logic centralized in core schema
4. **Type Safety**: Core functions work with PostgreSQL types, not JSONB
5. **Modularity**: Clear separation between input handling and business logic
6. **Performance**: Reduced JSONB parsing in business logic layer

**Testing benefits:**
```sql
-- Easy to test core function with typed parameters
SELECT * FROM core.create_user(
    '11111111-1111-1111-1111-111111111111'::UUID,
    '22222222-2222-2222-2222-222222222222'::UUID,
    ROW('test@example.com', 'Test User', 'Bio text', NULL, 'member')::app.type_user_input,
    '{"email": "test@example.com", "name": "Test User"}'::JSONB
);
```

### 6. Document Complex Function Interactions

**Core functions calling other core functions:**
```sql
CREATE OR REPLACE FUNCTION core.publish_post(
    input_pk_organization UUID,
    input_published_by UUID,
    input_data app.type_publish_post_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_post_data JSONB;
    v_author_id UUID;
BEGIN
    -- Get post data
    SELECT data, (data->>'author_id')::UUID
    INTO v_post_data, v_author_id
    FROM public.tv_post
    WHERE id = input_data.post_id;

    -- Business validation
    IF v_post_data IS NULL THEN
        RETURN core.log_and_return_mutation(..., 'noop:not_found', ...);
    END IF;

    -- Update post status
    UPDATE tenant.tb_post
    SET data = data || jsonb_build_object('is_published', true, 'published_at', NOW())
    WHERE pk_post = input_data.post_id;

    -- Call other core functions for side effects
    PERFORM core.update_author_published_count(v_author_id);
    PERFORM core.send_publication_notifications(input_data.post_id, v_author_id);
    PERFORM core.update_tag_usage_stats(v_post_data->'tags');

    -- Return success
    RETURN core.log_and_return_mutation(...);
END;
$$ LANGUAGE plpgsql;
```

### 7. Document Migration Pattern

**Converting monolithic functions:**

**Before (monolithic):**
```sql
CREATE OR REPLACE FUNCTION fn_create_user(input_data JSONB)
RETURNS JSONB AS $$
DECLARE
    v_email TEXT;
    v_name TEXT;
    v_user_id UUID;
BEGIN
    -- Mixed input handling and business logic
    v_email := input_data->>'email';
    v_name := input_data->>'name';

    IF v_email IS NULL THEN
        RETURN jsonb_build_object('success', false, 'error', 'Email required');
    END IF;

    IF EXISTS (SELECT 1 FROM users WHERE email = v_email) THEN
        RETURN jsonb_build_object('success', false, 'error', 'Email exists');
    END IF;

    INSERT INTO users (email, name) VALUES (v_email, v_name)
    RETURNING id INTO v_user_id;

    RETURN jsonb_build_object('success', true, 'user_id', v_user_id);
END;
$$ LANGUAGE plpgsql;
```

**After (app/core split):**
```sql
-- App function: Input handling only
CREATE OR REPLACE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_user_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);
    RETURN core.create_user(input_pk_organization, input_created_by, v_input, input_payload);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Business logic only
CREATE OR REPLACE FUNCTION core.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_data app.type_user_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
-- Business logic implementation here
$$ LANGUAGE plpgsql;
```

### 8. Documentation Structure

Add comprehensive section to existing file:

**Location**: `docs/mutations/postgresql-function-based.md`
**New section**: "Enterprise Function Architecture: App/Core Split Pattern" (after line ~200)

**Subsections:**
1. **Overview** - Why split functions into layers?
2. **Architecture Diagram** - Visual representation
3. **App Schema Functions** - Input handling layer
4. **Core Schema Functions** - Business logic layer
5. **Type Definitions** - Input type patterns
6. **Function Interactions** - Core calling core
7. **Benefits** - Testability, maintainability, reusability
8. **Migration Guide** - Converting existing functions
9. **Best Practices** - Do's and don'ts
10. **Testing Patterns** - How to test each layer
11. **Performance Considerations** - Optimization strategies
12. **Troubleshooting** - Common issues

### 9. Integration with Existing Patterns

**Reference other FraiseQL patterns:**
- Mutation result pattern (for return values)
- Multi-tenancy (tenant context passing)
- CQRS (command/query separation)
- Authentication (security definer usage)

**Update existing examples:**
- Convert basic examples to use app/core split
- Show both patterns for comparison
- Highlight when to use each approach

## Success Criteria

After implementation:
- [ ] Comprehensive app/core split documentation added
- [ ] Clear architectural diagrams included
- [ ] Complete migration guide provided
- [ ] All code examples use split pattern
- [ ] Benefits and trade-offs explained
- [ ] Testing strategies documented
- [ ] Integration with other patterns shown

## File Location

**Modify existing**: `docs/mutations/postgresql-function-based.md`
- Add new section around line 200-300
- Update existing examples to reference split pattern
- Add cross-references to related patterns

## Implementation Methodology

### Development Workflow

**Critical: Incremental Documentation Strategy**

This pattern modifies existing documentation and requires careful integration:

1. **Planning Commit** (5-10 minutes)
   ```bash
   # Document current state and plan integration points
   git add docs/mutations/postgresql-function-based.md
   git commit -m "docs: plan app/core function split integration

   - Add TODO comments for integration points
   - Mark existing examples for conversion
   - Plan new section placement
   - References #[issue-number]"
   ```

2. **Architecture Foundation Commit** (20-30 minutes)
   ```bash
   # Add core architectural concepts
   git add docs/mutations/postgresql-function-based.md
   git commit -m "docs: add app/core split architecture overview

   - Document two-layer function philosophy
   - Add architectural diagram
   - Define separation of concerns
   - Show basic function signatures"
   ```

3. **App Layer Documentation Commit** (25-35 minutes)
   ```bash
   # Complete app schema function patterns
   git add docs/mutations/postgresql-function-based.md
   git commit -m "docs: document app layer function patterns

   - Show JSONB input handling
   - Document type conversion patterns
   - Include validation examples
   - Add security definer usage"
   ```

4. **Core Layer Documentation Commit** (30-40 minutes)
   ```bash
   # Complete core schema function patterns
   git add docs/mutations/postgresql-function-based.md
   git commit -m "docs: document core layer business logic

   - Show typed parameter handling
   - Document business logic patterns
   - Include core-to-core function calls
   - Add transaction handling examples"
   ```

5. **Integration Examples Commit** (20-30 minutes)
   ```bash
   # Update existing examples to use split pattern
   git add docs/mutations/postgresql-function-based.md
   git commit -m "docs: update examples for app/core split pattern

   - Convert monolithic examples to split pattern
   - Add before/after migration examples
   - Update best practices section
   - Add testing guidance for both layers"
   ```

6. **Cross-Reference Integration Commit** (10-15 minutes)
   ```bash
   # Add cross-references and finalize
   git add docs/mutations/postgresql-function-based.md docs/mutations/index.md
   git commit -m "docs: complete app/core split integration

   - Add cross-references to related patterns
   - Update function-based mutations index
   - Finalize troubleshooting section
   - Ready for review"
   ```

### Integration Best Practices

**Before modifying existing file:**
```bash
# Create backup of current state
cp docs/mutations/postgresql-function-based.md docs/mutations/postgresql-function-based.md.backup

# Work on a feature branch for safety
git checkout -b docs/app-core-split-pattern
```

**During integration:**
- Preserve all existing examples (convert, don't delete)
- Add "Before/After" sections for comparisons
- Use comments to mark integration points
- Maintain existing heading structure

**Validation steps:**
- [ ] Existing content unchanged except for improvements
- [ ] New examples follow PrintOptim patterns exactly
- [ ] All SQL examples have correct syntax
- [ ] Cross-references link properly
- [ ] Document structure remains logical

### Risk Management

**For large file modifications:**
```bash
# If integration becomes complex, split into smaller changes
git stash  # Save current work
git commit -m "docs: intermediate app/core integration checkpoint"
git stash pop  # Continue with saved work
```

**Quality checks:**
```bash
# Test documentation build frequently
mkdocs serve
# Check for broken links
# Validate SQL syntax in separate database
# Review diff before each commit: git diff --cached
```

## Dependencies

Should integrate with:
- `mutation-result-pattern.md` - Return value structure
- `noop-handling-pattern.md` - How both layers handle NOOPs
- `../advanced/multi-tenancy.md` - Tenant context in both layers
- `../testing/mutations.md` - Testing both app and core functions

## Estimated Effort

**Medium-large effort** - Significant addition to existing documentation:
- Major new section (500-700 lines)
- Multiple detailed examples
- Architectural diagrams
- Migration guidance

**Integration work:**
- Update existing examples throughout document
- Add cross-references
- Update best practices section
