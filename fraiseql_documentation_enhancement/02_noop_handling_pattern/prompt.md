# NOOP Handling Pattern Documentation Prompt

## Context

This prompt is designed to generate comprehensive documentation for FraiseQL's NOOP (No Operation) handling pattern. NOOP responses are a critical part of FraiseQL's mutation result pattern, providing graceful handling of scenarios where mutations cannot be completed due to validation failures, business rule violations, or idempotent operations.

## Current State

**Status: MISSING (0% coverage)**
- FraiseQL has no standardized NOOP handling
- No idempotency patterns documented
- Edge cases often result in errors instead of graceful handling
- No deduplication strategies

## Target Documentation

Create new documentation file: `docs/mutations/noop-handling-pattern.md`

## Implementation Requirements

### 1. Document Core NOOP Philosophy

**What is NOOP handling:**
- Operations that would have no effect return success without modification
- Idempotent operations - safe to retry
- Graceful handling of edge cases
- Client-friendly responses instead of errors

**When to use NOOP:**
- Duplicate creation attempts
- Updates with identical values
- Operations on non-existent entities
- Invalid state transitions
- Business rule violations

### 2. Document NOOP Status Codes

**Standard NOOP status codes:**
```sql
-- Creation NOOPs
'noop:already_exists'        -- Entity exists with same identifiers
'noop:duplicate_detected'    -- Exact duplicate found

-- Update NOOPs
'noop:not_found'            -- Entity doesn't exist
'noop:no_changes'           -- Update with identical values
'noop:invalid_state'        -- Entity in wrong state for operation

-- Deletion NOOPs
'noop:already_deleted'      -- Entity already deleted
'noop:cannot_delete_referenced' -- Has dependent entities
'noop:cannot_delete_protected'  -- Protected from deletion

-- Validation NOOPs
'noop:invalid_[field]'      -- Specific field validation failure
'noop:missing_[entity]'     -- Required reference missing
'noop:permission_denied'    -- Insufficient permissions

-- Business Rule NOOPs
'noop:business_rule_[rule]' -- Business rule violation
'noop:workflow_violation'   -- Workflow state violation
```

### 3. Document NOOP Response Structure

**NOOP response pattern:**
```sql
-- NOOP responses include current state
RETURN core.log_and_return_mutation(
    input_pk_organization,
    input_user_id,
    'entity_type',
    v_existing_id,           -- ID of existing entity
    'NOOP',                  -- Modification type
    'noop:already_exists',   -- Specific status
    ARRAY[]::TEXT[],         -- No fields changed
    'Entity already exists with this identifier',
    v_existing_data,         -- Current state (before)
    v_existing_data,         -- Unchanged state (after)
    jsonb_build_object(
        'trigger', 'api_create',
        'reason', 'duplicate_identifier',
        'existing_id', v_existing_id,
        'requested_data', input_payload,
        'matched_fields', ARRAY['identifier', 'organization']
    )
);
```

### 4. Document Idempotency Patterns

**Create operation idempotency:**
```sql
-- Check for existing entity with same business key
IF EXISTS (
    SELECT 1 FROM tenant.tb_contract
    WHERE pk_organization = input_pk_organization
    AND data->>'identifier' = v_input.identifier
) THEN
    -- Return existing entity instead of error
    SELECT pk_contract, data INTO v_existing_id, v_existing_data
    FROM tenant.tb_contract
    WHERE pk_organization = input_pk_organization
    AND data->>'identifier' = v_input.identifier;

    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_created_by,
        'contract',
        v_existing_id,
        'NOOP',
        'noop:already_exists',
        ARRAY[]::TEXT[],
        format('Contract with identifier %s already exists', v_input.identifier),
        v_existing_data,
        v_existing_data,
        jsonb_build_object(
            'trigger', 'api_create',
            'idempotent_match', true,
            'existing_identifier', v_input.identifier
        )
    );
END IF;
```

**Update operation idempotency:**
```sql
-- Compare current values with requested changes
v_changed_fields := ARRAY(
    SELECT key
    FROM jsonb_each(input_payload)
    WHERE value IS DISTINCT FROM (v_current_data->key)
);

-- If no actual changes, return NOOP
IF array_length(v_changed_fields, 1) IS NULL THEN
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_updated_by,
        'contract',
        input_pk_contract,
        'NOOP',
        'noop:no_changes',
        ARRAY[]::TEXT[],
        'No changes detected',
        v_current_data,
        v_current_data,
        jsonb_build_object(
            'trigger', 'api_update',
            'requested_changes', input_payload,
            'identical_values', true
        )
    );
END IF;
```

### 5. Document GraphQL Integration

**Success vs NOOP handling:**
```python
@fraiseql.success
class CreateContractSuccess:
    contract: Contract
    message: str = "Contract created successfully"
    was_noop: bool = False

@fraiseql.success
class CreateContractNoop:
    """NOOP result - contract already exists"""
    existing_contract: Contract
    message: str
    noop_reason: str
    was_noop: bool = True

@fraiseql.failure
class CreateContractError:
    message: str
    error_code: str

@fraiseql.mutation
async def create_contract(
    info: GraphQLResolveInfo,
    input: CreateContractInput
) -> CreateContractSuccess | CreateContractNoop | CreateContractError:
    """Create contract with NOOP handling."""

    result = await db.call_function("app.create_contract", ...)

    status = result.get("status", "")

    if status == "new":
        return CreateContractSuccess(
            contract=Contract.from_dict(result["object_data"]),
            message=result["message"]
        )
    elif status.startswith("noop:"):
        return CreateContractNoop(
            existing_contract=Contract.from_dict(result["object_data"]),
            message=result["message"],
            noop_reason=status.replace("noop:", "")
        )
    else:
        return CreateContractError(
            message=result.get("message", "Operation failed"),
            error_code="OPERATION_FAILED"
        )
```

### 6. Document Client Handling

**Frontend handling of NOOP responses:**
```typescript
// GraphQL client handling
const result = await client.mutate({
  mutation: CREATE_CONTRACT,
  variables: { input: contractData }
});

const response = result.data.createContract;

switch (response.__typename) {
  case 'CreateContractSuccess':
    showSuccess(`Contract created: ${response.contract.identifier}`);
    break;

  case 'CreateContractNoop':
    // NOOP - show existing entity
    showInfo(`Contract ${response.existingContract.identifier} already exists`);
    // Optionally navigate to existing contract
    router.push(`/contracts/${response.existingContract.id}`);
    break;

  case 'CreateContractError':
    showError(response.message);
    break;
}
```

### 7. Document Deduplication Strategies

**Multi-field deduplication:**
```sql
-- Check for duplicates across multiple business keys
DECLARE
    v_duplicate_check RECORD;
    v_match_type TEXT;
BEGIN
    -- Check primary identifier
    SELECT pk_contract, 'identifier' INTO v_duplicate_check, v_match_type
    FROM tenant.tb_contract
    WHERE pk_organization = input_pk_organization
    AND data->>'identifier' = v_input.identifier
    LIMIT 1;

    -- Check secondary keys if primary not found
    IF v_duplicate_check.pk_contract IS NULL THEN
        SELECT pk_contract, 'external_id' INTO v_duplicate_check, v_match_type
        FROM tenant.tb_contract
        WHERE pk_organization = input_pk_organization
        AND data->>'external_id' = v_input.external_id
        AND v_input.external_id IS NOT NULL
        LIMIT 1;
    END IF;

    -- Handle duplicate based on match type
    IF v_duplicate_check.pk_contract IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            -- ... NOOP response with match details
            jsonb_build_object(
                'trigger', 'api_create',
                'match_type', v_match_type,
                'matched_field', CASE v_match_type
                    WHEN 'identifier' THEN v_input.identifier
                    WHEN 'external_id' THEN v_input.external_id
                END
            )
        );
    END IF;
END;
```

### 8. Document Error vs NOOP Decision Matrix

**When to return NOOP vs Error:**

| Scenario | Response | Rationale |
|----------|----------|-----------|
| Create duplicate by identifier | NOOP | Client might retry, return existing |
| Create with invalid FK | Error | Data integrity issue, needs fixing |
| Update non-existent entity | NOOP | Idempotent - no effect achieved |
| Update with no changes | NOOP | Idempotent - desired state achieved |
| Delete already deleted | NOOP | Idempotent - desired state achieved |
| Delete with dependents | NOOP* | Business rule, might be temporary |
| Invalid input format | Error | Client needs to fix input |
| Permission denied | NOOP | Business rule, predictable |

*Can be configured to return Error for strict validation

### 9. Documentation Structure

Create comprehensive sections:
1. **Overview** - What is NOOP handling and why use it?
2. **Status Codes** - All NOOP status codes and meanings
3. **Idempotency Principles** - Making operations safe to retry
4. **Implementation Patterns** - SQL function examples
5. **GraphQL Integration** - Response type patterns
6. **Client Handling** - How frontends should handle NOOPs
7. **Deduplication Strategies** - Multi-field duplicate detection
8. **Decision Matrix** - When to use NOOP vs Error
9. **Performance Considerations** - Efficient duplicate checking
10. **Testing Patterns** - How to test NOOP scenarios
11. **Best Practices** - Do's and don'ts
12. **Troubleshooting** - Common issues and solutions

## Success Criteria

After implementation:
- [ ] Complete NOOP handling documentation created
- [ ] All status codes documented with examples
- [ ] Idempotency patterns clearly explained
- [ ] GraphQL integration patterns shown
- [ ] Client handling examples provided
- [ ] Testing guidance included
- [ ] Follows FraiseQL documentation style

## File Location

Create: `docs/mutations/noop-handling-pattern.md`

Update: `docs/mutations/index.md` to include link

## Implementation Methodology

### Development Workflow

**Critical: Commit Incrementally for Large Documentation**

Break this comprehensive guide into manageable commits:

1. **Foundation Commit** (10-15 minutes)
   ```bash
   # Establish document structure and core concepts
   git add docs/mutations/noop-handling-pattern.md
   git commit -m "docs: initialize NOOP handling pattern guide

   - Add document structure and overview
   - Define NOOP philosophy and benefits
   - List all standard NOOP status codes
   - References #[issue-number]"
   ```

2. **SQL Patterns Commit** (25-35 minutes)
   ```bash
   # Complete PostgreSQL function examples
   git add docs/mutations/noop-handling-pattern.md
   git commit -m "docs: add NOOP SQL implementation patterns

   - Show idempotency checking logic
   - Document create/update/delete NOOP handling
   - Include deduplication strategies
   - Add mutation result integration"
   ```

3. **GraphQL Integration Commit** (20-30 minutes)
   ```bash
   # Complete GraphQL resolver patterns
   git add docs/mutations/noop-handling-pattern.md
   git commit -m "docs: add GraphQL NOOP response handling

   - Document Success/NOOP/Error response types
   - Show resolver parsing logic
   - Include type definitions for NOOP responses"
   ```

4. **Client Patterns Commit** (15-25 minutes)
   ```bash
   # Add frontend handling examples
   git add docs/mutations/noop-handling-pattern.md
   git commit -m "docs: add client-side NOOP handling patterns

   - Show TypeScript/JavaScript NOOP handling
   - Document UX patterns for NOOP responses
   - Include retry and navigation strategies"
   ```

5. **Decision Matrix & Guidelines Commit** (20-25 minutes)
   ```bash
   # Complete decision guidance and best practices
   git add docs/mutations/noop-handling-pattern.md docs/mutations/index.md
   git commit -m "docs: complete NOOP pattern with guidelines

   - Add NOOP vs Error decision matrix
   - Document testing patterns
   - Include performance considerations
   - Add troubleshooting section
   - Update mutations index"
   ```

### Progress Validation

After each commit:
- [ ] Build documentation locally (`mkdocs serve`)
- [ ] Validate SQL syntax in examples
- [ ] Test GraphQL schema definitions
- [ ] Check cross-references and links
- [ ] Ensure examples follow PrintOptim patterns

### Risk Mitigation

**For complex examples:**
```bash
# Test SQL examples in separate file first
# Create temp_noop_examples.sql
# Test in database before adding to docs

# Validate GraphQL types
# Use GraphQL validator or schema checker
```

**Recovery strategy:**
```bash
# If commit has issues, amend instead of new commit
git add -A
git commit --amend --no-edit

# Or create fixup commit for small changes
git commit -m "fixup: correct NOOP status code example"
```

## Dependencies

Should reference:
- `mutation-result-pattern.md` - Uses mutation result structure
- `postgresql-function-based.md` - Function implementation patterns
- `../testing/mutations.md` - Testing NOOP scenarios

## Estimated Effort

**Large effort** - Comprehensive pattern requiring:
- Detailed explanation of idempotency concepts
- Multiple SQL and GraphQL examples
- Client-side handling examples
- Decision-making guidance

Target: 700-900 lines of documentation
