# Prompt: Implement Mutation Result Pattern Documentation

## Objective

Create comprehensive documentation for FraiseQL's standardized mutation result pattern, based on PrintOptim Backend's `app.mutation_result` type system. This pattern is **critical** for enterprise applications as it provides consistent mutation responses, comprehensive metadata, and audit trails.

## Current State

**Status: MISSING (0% coverage)**
- FraiseQL has no standardized mutation response structure
- Each mutation returns ad-hoc JSON objects
- No consistent error handling or metadata patterns
- Missing field-level change tracking

## Target Documentation

Create new documentation file: `docs/mutations/mutation-result-pattern.md`

## Implementation Requirements

### 1. Document Core Type Structure

**Define app.mutation_result type:**
```sql
CREATE TYPE app.mutation_result AS (
    id UUID,                    -- Entity primary key
    updated_fields TEXT[],      -- Array of changed field names
    status TEXT,                -- Status: new, updated, deleted, noop:*
    message TEXT,               -- Human-readable message
    object_data JSONB,          -- Complete entity from tv_* or v_* view
    extra_metadata JSONB        -- Debug context and audit information
);
```

### 2. Document Status Semantics

**Success statuses:**
- `'new'` - Entity was created
- `'updated'` - Entity was modified
- `'deleted'` - Entity was removed

**NOOP statuses (with prefix):**
- `'noop:already_exists'` - Duplicate detected
- `'noop:not_found'` - Entity doesn't exist
- `'noop:no_changes'` - Update with identical values
- `'noop:invalid_[field]'` - Validation failure
- `'noop:cannot_delete_[reason]'` - Deletion blocked

### 3. Document Logging Function

**Core function pattern:**
```sql
CREATE FUNCTION core.log_and_return_mutation(
    input_pk_organization UUID,
    input_actor UUID,
    input_entity_type TEXT,
    input_entity_id UUID,
    input_modification_type TEXT,  -- INSERT, UPDATE, DELETE, NOOP
    input_change_status TEXT,      -- new, updated, noop:*
    input_fields TEXT[],
    input_message TEXT,
    input_payload_before JSONB,
    input_payload_after JSONB,
    input_extra_metadata JSONB DEFAULT '{}'::JSONB
) RETURNS app.mutation_result;
```

### 4. Document GraphQL Integration

Show how FraiseQL mutations automatically handle `app.mutation_result`:

**Python resolver pattern:**
```python
@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> CreateUserResult:
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    # Call PostgreSQL function (returns app.mutation_result)
    result = await db.call_function(
        "app.create_user",
        input_pk_organization=tenant_id,
        input_created_by=info.context["user_id"],
        input_payload=input.to_dict()
    )

    # Parse mutation_result structure
    if result["status"].startswith("noop:"):
        return CreateUserError(
            message=result["message"],
            error_code=result["status"].replace("noop:", "").upper()
        )
    elif result["status"] in ["new", "updated"]:
        return CreateUserSuccess(
            user=User.from_dict(result["object_data"]),
            message=result["message"]
        )
```

### 5. Document Benefits

**Enterprise advantages:**
- **Audit compliance** - Complete change history
- **Debugging support** - Rich metadata for troubleshooting
- **Consistent responses** - Standardized across all mutations
- **Field tracking** - Know exactly what changed
- **NOOP handling** - Graceful handling of edge cases

### 6. Documentation Structure

Create sections:
1. **Overview** - What is the mutation result pattern?
2. **Type Definition** - SQL type structure
3. **Status Codes** - All possible status values
4. **Logging Function** - Central logging mechanism
5. **GraphQL Integration** - How resolvers use this
6. **Metadata Patterns** - What goes in extra_metadata
7. **Change Tracking** - How updated_fields works
8. **Examples** - Complete working examples
9. **Migration Guide** - Converting existing mutations
10. **Best Practices** - Do's and don'ts
11. **Troubleshooting** - Common issues

### 7. Code Examples

Include complete working examples:
- Simple create mutation
- Update with change tracking
- NOOP handling scenario
- Complex business logic mutation
- Error handling patterns

### 8. Integration Points

Document how this integrates with:
- FraiseQL's existing mutation decorators
- Authentication and authorization
- Multi-tenant patterns
- Cache invalidation
- Audit logging

## Success Criteria

After implementation:
- [ ] Complete documentation file created
- [ ] All SQL patterns documented with examples
- [ ] GraphQL integration patterns shown
- [ ] Migration guide for existing code
- [ ] Troubleshooting section included
- [ ] Matches FraiseQL's documentation style and quality

## Style Guidelines

Follow FraiseQL's documentation standards:
- Use markdown with proper headings
- Include comprehensive code examples
- Add performance and security considerations
- Provide troubleshooting guidance
- Link to related documentation
- Include "See Also" sections

## File Location

Create: `docs/mutations/mutation-result-pattern.md`

Update: `docs/mutations/index.md` to include link to new documentation

## Estimated Effort

**Large effort** - This is a comprehensive new pattern requiring:
- Detailed explanation of enterprise concepts
- Complete SQL and Python examples
- Integration with existing FraiseQL patterns
- Migration guidance for users

Target: 800-1000 lines of documentation (similar to existing FraiseQL pattern docs)

## Implementation Methodology

### Development Workflow

**Critical: Commit Early and Often**

This is a large documentation task that should be broken into multiple commits:

1. **Initial Structure Commit** (5-10 minutes)
   ```bash
   # Create file with basic structure and headings
   git add docs/mutations/mutation-result-pattern.md
   git commit -m "docs: add mutation result pattern structure

   - Add main headings and sections
   - Include placeholder content
   - References #[issue-number]"
   ```

2. **Type Definition Commit** (15-20 minutes)
   ```bash
   # Complete SQL type definitions and basic examples
   git add docs/mutations/mutation-result-pattern.md
   git commit -m "docs: add mutation result type definitions

   - Define app.mutation_result type structure
   - Document all status codes with examples
   - Add core.log_and_return_mutation signature"
   ```

3. **GraphQL Integration Commit** (20-30 minutes)
   ```bash
   # Complete Python resolver patterns
   git add docs/mutations/mutation-result-pattern.md
   git commit -m "docs: add GraphQL resolver integration patterns

   - Show mutation result parsing in resolvers
   - Document success/error type mappings
   - Include complete resolver examples"
   ```

4. **Examples and Patterns Commit** (30-40 minutes)
   ```bash
   # Add comprehensive working examples
   git add docs/mutations/mutation-result-pattern.md
   git commit -m "docs: add mutation result pattern examples

   - Complete create/update/delete examples
   - Show NOOP handling scenarios
   - Include metadata usage patterns"
   ```

5. **Best Practices Commit** (15-20 minutes)
   ```bash
   # Complete troubleshooting and best practices
   git add docs/mutations/mutation-result-pattern.md docs/mutations/index.md
   git commit -m "docs: complete mutation result pattern guide

   - Add troubleshooting section
   - Document best practices and anti-patterns
   - Update mutations index with new pattern
   - Ready for review"
   ```

### Quality Checkpoints

After each commit, verify:
- [ ] Documentation builds without errors
- [ ] Code examples have correct syntax
- [ ] Cross-references link properly
- [ ] Follows FraiseQL style guide
- [ ] Examples match PrintOptim patterns

### Rollback Strategy

If issues arise:
```bash
# Rollback to last working commit
git reset --soft HEAD~1  # Keep changes staged
# OR
git reset --hard HEAD~1  # Discard all changes
```

### Testing Documentation

Before final commit:
```bash
# Test documentation build
mkdocs serve
# Navigate to new pages and verify rendering
# Test all code examples for syntax
# Verify cross-references work
```

## Dependencies

Should reference:
- `docs/mutations/postgresql-function-based.md` - Existing function documentation
- `docs/advanced/multi-tenancy.md` - Tenant context patterns
- `docs/advanced/cqrs.md` - Command-query separation
