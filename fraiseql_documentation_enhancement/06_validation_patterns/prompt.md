# Prompt: Implement Validation Patterns Documentation

## Objective

Create comprehensive documentation for FraiseQL's validation patterns, covering **multi-layer validation strategies** that ensure data integrity, business rule compliance, and user-friendly error handling. This pattern is essential for robust enterprise applications.

## Current State

**Status: BASIC COVERAGE (30% coverage)**
- Basic validation examples in function documentation
- No systematic validation pattern documentation
- Missing multi-layer validation strategy
- No comprehensive error handling for validation

## Target Documentation

Create new documentation file: `docs/mutations/validation-patterns.md`

## Implementation Requirements

### 1. Document Multi-Layer Validation Architecture

**Four-layer validation strategy:**
```
1. GraphQL Schema Validation (Type safety, required fields)
   ↓
2. App Layer Validation (Input sanitization, basic checks)
   ↓
3. Core Layer Validation (Business rules, complex constraints)
   ↓
4. Database Constraints (Data integrity, referential constraints)
```

**Each layer's responsibilities:**
- **GraphQL**: Type safety, nullability, basic format validation
- **App**: Input sanitization, JSONB parsing, preliminary checks
- **Core**: Business logic validation, cross-entity constraints
- **Database**: Data integrity, foreign keys, unique constraints

### 2. Document GraphQL Schema Validation

**Type-based validation:**
```python
from typing import Optional, Annotated
from pydantic import Field, EmailStr
import re

@fraiseql.input
class CreateUserInput:
    """User creation input with built-in validation."""

    # Required fields with type validation
    email: EmailStr  # Built-in email format validation
    name: Annotated[str, Field(min_length=2, max_length=100)]

    # Optional fields with constraints
    bio: Optional[Annotated[str, Field(max_length=1000)]] = None
    phone: Optional[Annotated[str, Field(pattern=r'^\+?[\d\s\-\(\)]+$')]] = None

    # Custom validation
    @validator('name')
    def validate_name(cls, v):
        if not v.strip():
            raise ValueError('Name cannot be empty or only whitespace')
        if any(char in v for char in '<>{}[]'):
            raise ValueError('Name contains invalid characters')
        return v.strip()

    @validator('phone')
    def validate_phone(cls, v):
        if v is None:
            return v
        # Remove all non-digits for length check
        digits_only = re.sub(r'\D', '', v)
        if len(digits_only) < 10 or len(digits_only) > 15:
            raise ValueError('Phone number must be 10-15 digits')
        return v

@fraiseql.input
class UpdateContractInput:
    """Contract update with conditional validation."""

    name: Optional[str] = None
    start_date: Optional[date] = None
    end_date: Optional[date] = None
    value: Optional[Annotated[Decimal, Field(ge=0, le=Decimal('999999.99'))]] = None

    @root_validator
    def validate_date_range(cls, values):
        start_date = values.get('start_date')
        end_date = values.get('end_date')

        if start_date and end_date and start_date >= end_date:
            raise ValueError('Start date must be before end date')

        return values
```

### 3. Document App Layer Validation

**Input sanitization and preliminary validation:**
```sql
-- App layer: Input sanitization and basic validation
CREATE OR REPLACE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_user_input;
    v_validation_errors JSONB := '{}'::JSONB;
BEGIN
    -- Parse and validate JSONB structure
    BEGIN
        v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);
    EXCEPTION WHEN OTHERS THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            NULL,
            'NOOP',
            'noop:invalid_input',
            ARRAY[]::TEXT[],
            'Invalid input format: ' || SQLERRM,
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_create',
                'validation_layer', 'app',
                'error_type', 'json_parse_error',
                'raw_input', input_payload
            )
        );
    END;

    -- Required field validation
    IF v_input.email IS NULL OR length(trim(v_input.email)) = 0 THEN
        v_validation_errors := v_validation_errors ||
            jsonb_build_object('email', 'Email is required');
    END IF;

    IF v_input.name IS NULL OR length(trim(v_input.name)) = 0 THEN
        v_validation_errors := v_validation_errors ||
            jsonb_build_object('name', 'Name is required');
    END IF;

    -- Basic format validation
    IF v_input.email IS NOT NULL AND NOT v_input.email ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$' THEN
        v_validation_errors := v_validation_errors ||
            jsonb_build_object('email', 'Invalid email format');
    END IF;

    -- Length validation
    IF v_input.name IS NOT NULL AND length(v_input.name) > 100 THEN
        v_validation_errors := v_validation_errors ||
            jsonb_build_object('name', 'Name must be 100 characters or less');
    END IF;

    -- Return validation errors if any
    IF jsonb_object_keys(v_validation_errors) IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            NULL,
            'NOOP',
            'noop:validation_failed',
            ARRAY[]::TEXT[],
            'Input validation failed',
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_create',
                'validation_layer', 'app',
                'field_errors', v_validation_errors,
                'validated_input', row_to_json(v_input)
            )
        );
    END IF;

    -- Input sanitization
    v_input.name := trim(v_input.name);
    v_input.email := lower(trim(v_input.email));
    v_input.bio := CASE
        WHEN v_input.bio IS NOT NULL THEN trim(v_input.bio)
        ELSE NULL
    END;

    -- Delegate to core layer with sanitized input
    RETURN core.create_user(
        input_pk_organization,
        input_created_by,
        v_input,
        input_payload
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

### 4. Document Core Layer Business Validation

**Complex business rule validation:**
```sql
-- Core layer: Business logic validation
CREATE OR REPLACE FUNCTION core.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_data app.type_user_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_org_settings JSONB;
    v_user_count INTEGER;
    v_domain_allowed BOOLEAN;
    v_email_domain TEXT;
    v_validation_context JSONB;
BEGIN
    -- Get organization settings for business rule validation
    SELECT data INTO v_org_settings
    FROM tenant.tb_organization
    WHERE pk_organization = input_pk_organization;

    v_validation_context := jsonb_build_object(
        'trigger', 'api_create',
        'validation_layer', 'core',
        'organization_settings', v_org_settings
    );

    -- Business Rule 1: Check organization user limit
    SELECT COUNT(*) INTO v_user_count
    FROM tenant.tb_user
    WHERE fk_customer_org = input_pk_organization
    AND deleted_at IS NULL;

    IF (v_org_settings->>'max_users')::INTEGER IS NOT NULL
       AND v_user_count >= (v_org_settings->>'max_users')::INTEGER THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            NULL,
            'NOOP',
            'noop:business_rule_user_limit',
            ARRAY[]::TEXT[],
            format('Organization has reached maximum user limit of %s',
                   v_org_settings->>'max_users'),
            NULL,
            NULL,
            v_validation_context || jsonb_build_object(
                'business_rule', 'max_users_exceeded',
                'current_count', v_user_count,
                'max_allowed', (v_org_settings->>'max_users')::INTEGER
            )
        );
    END IF;

    -- Business Rule 2: Email domain validation
    v_email_domain := split_part(input_data.email, '@', 2);

    IF v_org_settings->'allowed_email_domains' IS NOT NULL THEN
        SELECT v_email_domain = ANY(
            ARRAY(SELECT jsonb_array_elements_text(v_org_settings->'allowed_email_domains'))
        ) INTO v_domain_allowed;

        IF NOT v_domain_allowed THEN
            RETURN core.log_and_return_mutation(
                input_pk_organization,
                input_created_by,
                'user',
                NULL,
                'NOOP',
                'noop:business_rule_email_domain',
                ARRAY[]::TEXT[],
                format('Email domain %s is not allowed for this organization', v_email_domain),
                NULL,
                NULL,
                v_validation_context || jsonb_build_object(
                    'business_rule', 'email_domain_restricted',
                    'attempted_domain', v_email_domain,
                    'allowed_domains', v_org_settings->'allowed_email_domains'
                )
            );
        END IF;
    END IF;

    -- Business Rule 3: Check for existing user (duplicate prevention)
    IF EXISTS (
        SELECT 1 FROM tenant.tb_user
        WHERE fk_customer_org = input_pk_organization
        AND data->>'email' = input_data.email
        AND deleted_at IS NULL
    ) THEN
        -- Return existing user data for idempotent response
        DECLARE
            v_existing_user JSONB;
        BEGIN
            SELECT data INTO v_existing_user
            FROM public.tv_user
            WHERE tenant_id = input_pk_organization
            AND email = input_data.email;

            RETURN core.log_and_return_mutation(
                input_pk_organization,
                input_created_by,
                'user',
                (SELECT pk_user FROM tenant.tb_user
                 WHERE fk_customer_org = input_pk_organization
                 AND data->>'email' = input_data.email),
                'NOOP',
                'noop:already_exists',
                ARRAY[]::TEXT[],
                'User with this email already exists',
                v_existing_user,
                v_existing_user,
                v_validation_context || jsonb_build_object(
                    'business_rule', 'unique_email',
                    'existing_email', input_data.email
                )
            );
        END;
    END IF;

    -- Business Rule 4: Role validation based on creator permissions
    DECLARE
        v_creator_role TEXT;
        v_requested_role TEXT;
    BEGIN
        -- Get creator's role
        SELECT data->>'role' INTO v_creator_role
        FROM tenant.tb_user
        WHERE pk_user = input_created_by;

        v_requested_role := COALESCE(input_data.role, 'user');

        -- Only admins can create admin users
        IF v_requested_role = 'admin' AND v_creator_role != 'admin' THEN
            RETURN core.log_and_return_mutation(
                input_pk_organization,
                input_created_by,
                'user',
                NULL,
                'NOOP',
                'noop:business_rule_insufficient_permissions',
                ARRAY[]::TEXT[],
                'Only administrators can create admin users',
                NULL,
                NULL,
                v_validation_context || jsonb_build_object(
                    'business_rule', 'role_creation_permission',
                    'creator_role', v_creator_role,
                    'requested_role', v_requested_role
                )
            );
        END IF;
    END;

    -- All validations passed, proceed with creation
    DECLARE
        v_user_id UUID;
        v_payload_after JSONB;
    BEGIN
        INSERT INTO tenant.tb_user (
            pk_organization,
            data,
            created_at,
            created_by,
            updated_at,
            updated_by
        ) VALUES (
            input_pk_organization,
            jsonb_build_object(
                'email', input_data.email,
                'name', input_data.name,
                'bio', input_data.bio,
                'role', COALESCE(input_data.role, 'user'),
                'status', 'active',
                'email_verified', false
            ),
            NOW(),
            input_created_by,
            NOW(),
            input_created_by
        ) RETURNING pk_user INTO v_user_id;

        -- Get complete user data
        SELECT data INTO v_payload_after
        FROM public.tv_user
        WHERE id = v_user_id;

        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'user',
            v_user_id,
            'INSERT',
            'new',
            ARRAY['email', 'name', 'bio', 'role', 'status'],
            'User created successfully',
            NULL,
            v_payload_after,
            v_validation_context || jsonb_build_object(
                'business_rules_passed', ARRAY[
                    'user_limit_check',
                    'email_domain_check',
                    'duplicate_check',
                    'role_permission_check'
                ]
            )
        );
    END;
END;
$$ LANGUAGE plpgsql;
```

### 5. Document Database Constraint Validation

**Data integrity and referential constraints:**
```sql
-- Database constraints for data integrity
ALTER TABLE tenant.tb_user
ADD CONSTRAINT chk_user_email_format
CHECK (data->>'email' ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$');

ALTER TABLE tenant.tb_user
ADD CONSTRAINT chk_user_name_length
CHECK (length(data->>'name') >= 2 AND length(data->>'name') <= 100);

ALTER TABLE tenant.tb_user
ADD CONSTRAINT chk_user_role_valid
CHECK (data->>'role' IN ('user', 'admin', 'manager', 'viewer'));

-- Unique constraint on email per organization
CREATE UNIQUE INDEX uq_user_email_per_org
ON tenant.tb_user (fk_customer_org, (data->>'email'))
WHERE deleted_at IS NULL;

-- Foreign key constraints
ALTER TABLE tenant.tb_user
ADD CONSTRAINT fk_user_organization
FOREIGN KEY (fk_customer_org) REFERENCES tenant.tb_organization(pk_organization);

ALTER TABLE tenant.tb_user
ADD CONSTRAINT fk_user_created_by
FOREIGN KEY (created_by) REFERENCES tenant.tb_user(pk_user);
```

### 6. Document Validation Error Handling

**Structured error responses:**
```python
@fraiseql.failure
class ValidationError:
    """Comprehensive validation error response."""
    message: str
    error_code: str = "VALIDATION_ERROR"

    # Field-specific errors
    field_errors: Optional[dict[str, str]] = None

    # Validation metadata
    validation_layer: str  # 'graphql', 'app', 'core', 'database'
    failed_rules: Optional[list[str]] = None

    # Context for debugging
    validation_context: Optional[dict[str, Any]] = None

@fraiseql.mutation
async def create_user(
    info: GraphQLResolveInfo,
    input: CreateUserInput
) -> CreateUserSuccess | ValidationError | CreateUserError:
    """Create user with comprehensive validation error handling."""

    try:
        result = await db.call_function("app.create_user", ...)

        status = result.get("status", "")

        if status == "new":
            return CreateUserSuccess(
                user=User.from_dict(result["object_data"]),
                message=result["message"]
            )
        elif status.startswith("noop:validation_") or status.startswith("noop:business_rule_"):
            # Convert validation NOOPs to ValidationError
            metadata = result.get("extra_metadata", {})

            return ValidationError(
                message=result["message"],
                validation_layer=metadata.get("validation_layer", "unknown"),
                field_errors=metadata.get("field_errors"),
                failed_rules=metadata.get("failed_rules"),
                validation_context=metadata
            )
        elif status.startswith("noop:"):
            # Other NOOPs (like already_exists) become different success type
            return CreateUserNoop(
                existing_user=User.from_dict(result["object_data"]),
                message=result["message"],
                noop_reason=status.replace("noop:", "")
            )
        else:
            return CreateUserError(
                message=result.get("message", "User creation failed"),
                error_code="CREATION_FAILED"
            )

    except psycopg.IntegrityError as e:
        # Database constraint violations
        constraint_name = extract_constraint_name(str(e))

        return ValidationError(
            message=f"Data integrity violation: {constraint_name}",
            error_code="CONSTRAINT_VIOLATION",
            validation_layer="database",
            failed_rules=[constraint_name],
            validation_context={"constraint_error": str(e)}
        )
```

### 7. Document Cross-Entity Validation

**Validating relationships between entities:**
```sql
-- Cross-entity validation example: Contract with items
CREATE OR REPLACE FUNCTION core.validate_contract_items(
    input_contract_data JSONB,
    input_items JSONB[]
) RETURNS TABLE(
    is_valid BOOLEAN,
    error_message TEXT,
    failed_item_index INTEGER
) AS $$
DECLARE
    v_item JSONB;
    v_item_index INTEGER := 0;
    v_total_value DECIMAL := 0;
    v_contract_value DECIMAL;
BEGIN
    v_contract_value := (input_contract_data->>'total_value')::DECIMAL;

    -- Validate each item
    FOR v_item IN SELECT unnest(input_items)
    LOOP
        v_item_index := v_item_index + 1;

        -- Check if referenced product exists
        IF NOT EXISTS (
            SELECT 1 FROM tenant.tb_product
            WHERE pk_product = (v_item->>'product_id')::UUID
        ) THEN
            is_valid := FALSE;
            error_message := format('Product not found: %s', v_item->>'product_id');
            failed_item_index := v_item_index;
            RETURN NEXT;
            RETURN;
        END IF;

        -- Check quantity is positive
        IF (v_item->>'quantity')::INTEGER <= 0 THEN
            is_valid := FALSE;
            error_message := 'Item quantity must be positive';
            failed_item_index := v_item_index;
            RETURN NEXT;
            RETURN;
        END IF;

        -- Accumulate total value
        v_total_value := v_total_value +
            ((v_item->>'quantity')::INTEGER * (v_item->>'unit_price')::DECIMAL);
    END LOOP;

    -- Validate total value matches contract value
    IF ABS(v_total_value - v_contract_value) > 0.01 THEN
        is_valid := FALSE;
        error_message := format('Item total (%s) does not match contract value (%s)',
                               v_total_value, v_contract_value);
        failed_item_index := NULL;  -- Not specific to one item
        RETURN NEXT;
        RETURN;
    END IF;

    -- All validations passed
    is_valid := TRUE;
    error_message := NULL;
    failed_item_index := NULL;
    RETURN NEXT;
END;
$$ LANGUAGE plpgsql STABLE;
```

### 8. Document Validation Performance Patterns

**Optimizing validation queries:**
```sql
-- Efficient existence checks using indexes
CREATE INDEX idx_product_lookup ON tenant.tb_product (pk_product)
WHERE deleted_at IS NULL;

-- Batch validation for multiple items
CREATE OR REPLACE FUNCTION core.batch_validate_product_exists(
    input_product_ids UUID[]
) RETURNS UUID[] AS $$
BEGIN
    -- Return array of non-existent product IDs
    RETURN ARRAY(
        SELECT unnest(input_product_ids)
        EXCEPT
        SELECT pk_product
        FROM tenant.tb_product
        WHERE pk_product = ANY(input_product_ids)
        AND deleted_at IS NULL
    );
END;
$$ LANGUAGE plpgsql STABLE;

-- Validation with early exit for performance
CREATE OR REPLACE FUNCTION core.validate_with_early_exit(
    input_data JSONB
) RETURNS JSONB AS $$
DECLARE
    v_validation_result JSONB := '{"valid": true}'::JSONB;
BEGIN
    -- Most likely to fail validation first (performance optimization)
    IF (input_data->>'email') IS NULL THEN
        RETURN '{"valid": false, "error": "Email required", "field": "email"}'::JSONB;
    END IF;

    -- More expensive validations only if basic ones pass
    IF EXISTS (SELECT 1 FROM tenant.tb_user WHERE data->>'email' = input_data->>'email') THEN
        RETURN '{"valid": false, "error": "Email exists", "field": "email"}'::JSONB;
    END IF;

    -- Most expensive validations last
    -- ... complex business rule checks

    RETURN v_validation_result;
END;
$$ LANGUAGE plpgsql STABLE;
```

### 9. Documentation Structure

Create comprehensive sections:
1. **Overview** - Multi-layer validation strategy
2. **GraphQL Schema Validation** - Type safety and input validation
3. **App Layer Validation** - Input sanitization and basic checks
4. **Core Layer Validation** - Business rules and complex constraints
5. **Database Constraints** - Data integrity and referential constraints
6. **Error Handling** - Structured validation error responses
7. **Cross-Entity Validation** - Validating relationships
8. **Performance Patterns** - Optimizing validation queries
9. **Testing Validation** - How to test each validation layer
10. **Best Practices** - Validation design principles
11. **Common Patterns** - Reusable validation functions
12. **Troubleshooting** - Debugging validation issues

## Success Criteria

After implementation:
- [ ] Complete multi-layer validation documentation
- [ ] All validation layers covered with examples
- [ ] Structured error handling patterns shown
- [ ] Performance optimization strategies included
- [ ] Testing guidance provided
- [ ] Best practices documented
- [ ] Follows FraiseQL documentation style

## File Location

Create: `docs/mutations/validation-patterns.md`

Update: `docs/mutations/index.md` to include link

## Implementation Methodology

### Development Workflow

**Critical: Layered Validation Documentation Strategy**

Break this multi-layer validation pattern into systematic commits:

1. **Validation Architecture Commit** (20-25 minutes)
   ```bash
   # Establish multi-layer validation foundation
   git add docs/mutations/validation-patterns.md
   git commit -m "docs: initialize validation patterns guide

   - Define four-layer validation architecture
   - Document validation responsibility matrix
   - Show validation flow and layer interactions
   - Include validation philosophy and principles
   - References #[issue-number]"
   ```

2. **GraphQL and App Layer Commit** (25-35 minutes)
   ```bash
   # Complete input validation patterns
   git add docs/mutations/validation-patterns.md
   git commit -m "docs: add GraphQL and app layer validation

   - Document GraphQL schema validation with examples
   - Show app layer input sanitization patterns
   - Include format validation and basic checks
   - Add type safety and input transformation"
   ```

3. **Core Business Logic Commit** (35-45 minutes)
   ```bash
   # Complete business rule validation patterns
   git add docs/mutations/validation-patterns.md
   git commit -m "docs: add core layer business validation

   - Document complex business rule validation
   - Show state transition validation patterns
   - Include domain-specific validation functions
   - Add business constraint examples"
   ```

4. **Error Handling and Responses Commit** (25-35 minutes)
   ```bash
   # Complete validation error handling patterns
   git add docs/mutations/validation-patterns.md
   git commit -m "docs: add validation error handling patterns

   - Document structured validation error responses
   - Show GraphQL error type integration
   - Include field-specific error mapping
   - Add debugging and context information"
   ```

5. **Cross-Entity and Performance Commit** (30-40 minutes)
   ```bash
   # Complete advanced validation patterns
   git add docs/mutations/validation-patterns.md
   git commit -m "docs: add cross-entity and performance validation

   - Document relationship validation patterns
   - Show batch validation optimizations
   - Include early-exit performance strategies
   - Add constraint validation examples"
   ```

6. **Testing and Best Practices Commit** (20-25 minutes)
   ```bash
   # Complete with testing and finalization
   git add docs/mutations/validation-patterns.md docs/mutations/index.md
   git commit -m "docs: complete validation patterns guide

   - Add validation testing strategies
   - Include troubleshooting and debugging
   - Document validation best practices
   - Update mutations index with validation patterns
   - Ready for review"
   ```

### Quality Validation

After each commit:
- [ ] Build documentation (`mkdocs serve`)
- [ ] Validate all SQL validation function syntax
- [ ] Test GraphQL schema validation examples
- [ ] Verify error response structures
- [ ] Check performance optimization suggestions
- [ ] Ensure validation examples follow PrintOptim patterns

### Risk Management

**For complex business rules:**
```bash
# Test business rule validation logic separately
# Verify constraint examples work correctly
# Include edge case handling in examples
```

**For performance patterns:**
```bash
# Validate indexing recommendations
# Test batch validation performance claims
# Include realistic performance benchmarks
```

**Recovery strategy:**
```bash
# Complex validation examples should be tested
git add -p  # Stage working examples incrementally
git commit -m "docs: partial validation pattern implementation"
```

## Dependencies

Should reference:
- `postgresql-function-based.md` - Function implementation patterns
- `mutation-result-pattern.md` - Error response structures
- `noop-handling-pattern.md` - Validation NOOPs
- `../testing/mutations.md` - Testing validation logic

## Estimated Effort

**Large effort** - Complex enterprise pattern:
- Multi-layer architecture explanation
- Comprehensive validation examples
- Error handling strategies
- Performance optimization guidance

Target: 1000-1200 lines of documentation
