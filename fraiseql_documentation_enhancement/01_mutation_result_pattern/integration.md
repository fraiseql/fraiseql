# Mutation Result Pattern - Integration Guide

## Integration with Existing FraiseQL Documentation

### Files to Update

#### 1. Create New Documentation
**Primary file:** `docs/mutations/mutation-result-pattern.md`
- Complete implementation of the mutation result pattern
- 800-1000 lines following FraiseQL style
- Include all examples from examples.md

#### 2. Update Existing Files

**`docs/mutations/index.md`**
Add new section:
```markdown
## Advanced Patterns

- [**PostgreSQL Function-Based Mutations**](postgresql-function-based.md) - Implement business logic in the database
- [**Mutation Result Pattern**](mutation-result-pattern.md) - ✨ NEW: Standardized mutation responses and audit trails
- [**Error Handling**](../errors/index.md) - Handle mutations errors gracefully
```

**`docs/mutations/postgresql-function-based.md`**
Add cross-references in these sections:

1. **In "Return Type Patterns" section (around line 850):**
```markdown
### Enterprise Mutation Results

For production applications requiring audit trails and comprehensive metadata, consider using the [Mutation Result Pattern](mutation-result-pattern.md) which provides:

- Standardized response structure across all mutations
- Field-level change tracking
- Comprehensive audit metadata
- NOOP handling for idempotent operations

```sql
-- Instead of basic JSON return
RETURN jsonb_build_object('success', true, 'user', v_user_data);

-- Use mutation result pattern
RETURN core.log_and_return_mutation(
    input_pk_organization,
    input_created_by,
    'user',
    v_user_id,
    'INSERT',
    'new',
    ARRAY['name', 'email'],
    'User created successfully',
    NULL,
    v_user_data,
    jsonb_build_object('trigger', 'api_create')
);
```

See [Mutation Result Pattern](mutation-result-pattern.md) for complete implementation details.
```

2. **In "Best Practices" section (around line 1124):**
Add new best practice:
```markdown
11. **Use mutation result pattern**: For enterprise applications, implement standardized mutation responses with audit trails and metadata
```

**`docs/advanced/multi-tenancy.md`**
Add reference in "Best Practices" section:
```markdown
### Operational
- Automate tenant provisioning and deprovisioning
- Implement tenant-aware monitoring and alerting
- Plan for tenant data migration and archival
- Document tenant onboarding procedures
- **Use mutation result pattern** for audit compliance across tenants
```

**`docs/advanced/cqrs.md`**
Add reference in "Best Practices" section (around line 256):
```markdown
### Command Design
```python
# ✅ Good: Single responsibility with mutation result pattern
@fraiseql.mutation
async def approve_post(info, post_id: ID) -> ApprovePostResult:
    """Single command with standardized result structure."""
    result = await db.call_function("app.approve_post", ...)

    # Parse app.mutation_result response
    if result["status"] == "updated":
        return ApprovePostSuccess(
            post=Post.from_dict(result["object_data"]),
            message=result["message"],
            updated_fields=result["updated_fields"]
        )
```

See [Mutation Result Pattern](../mutations/mutation-result-pattern.md) for complete implementation.
```

#### 3. Update Example Code

**`examples/blog_api/mutations.py`**
Add comment at top referencing new pattern:
```python
"""Example blog API mutations using FraiseQL with CQRS.

Note: This example uses basic mutation patterns. For production applications,
consider implementing the Mutation Result Pattern for standardized responses,
audit trails, and comprehensive metadata.

See: docs/mutations/mutation-result-pattern.md
"""
```

**`examples/ecommerce_api/mutations.py`**
Similar comment addition.

### Navigation Updates

**`docs/index.md`** (if exists)
Add to mutations section:
```markdown
### Mutations
- [PostgreSQL Functions](mutations/postgresql-function-based.md)
- [Mutation Result Pattern](mutations/mutation-result-pattern.md) - Enterprise audit and metadata
- [Error Handling](errors/index.md)
```

**`mkdocs.yml`** (if using MkDocs)
Add navigation entry:
```yaml
nav:
  - Mutations:
    - mutations/index.md
    - mutations/postgresql-function-based.md
    - mutations/mutation-result-pattern.md
    - migrations/index.md
```

### Cross-Reference Strategy

#### Link FROM mutation-result-pattern.md TO:
- `../mutations/postgresql-function-based.md` - Foundation function patterns
- `../advanced/multi-tenancy.md` - Tenant context handling
- `../advanced/cqrs.md` - Command-query separation principles
- `../errors/index.md` - Error handling strategies
- `../testing/mutations.md` - Testing mutation patterns

#### Link TO mutation-result-pattern.md FROM:
- All mutation-related documentation
- Enterprise/production readiness guides
- Audit and compliance documentation
- Multi-tenancy guides
- CQRS implementation guides

### Style Consistency

#### Follow FraiseQL Documentation Patterns

1. **Section Structure:**
```markdown
---
← [Previous Page](link.md) | [Section Index](index.md) | [Next Page →](link.md)
---

# Title

> **In this section:** Brief description
> **Prerequisites:** Required knowledge
> **Time to complete:** Estimated reading time

Overview paragraph...

## Main Sections
...

## See Also

### Related Concepts
- [**Pattern Name**](link.md) - Brief description

### Implementation Guides
- [**Guide Name**](link.md) - Brief description

### Advanced Topics
- [**Topic Name**](link.md) - Brief description
```

2. **Code Block Formatting:**
```markdown
#### Pattern Name
```sql
-- SQL code with comments
CREATE FUNCTION ...;
```

```python
# Python code with explanations
@fraiseql.mutation
async def example(...):
    ...
```
</markdown>
```

3. **Mermaid Diagrams:**
Include architectural diagrams using mermaid for complex patterns.

4. **Warning/Info Boxes:**
```markdown
> **Note:** Important information
> **Warning:** Critical considerations
> **Prerequisites:** Required setup
```

### Quality Checklist

Before submitting:
- [ ] New documentation follows FraiseQL style guide
- [ ] All code examples are tested and working
- [ ] Cross-references are accurate and complete
- [ ] Navigation is updated consistently
- [ ] Search terms are included for discoverability
- [ ] Performance and security considerations included
- [ ] Troubleshooting section addresses common issues

### Migration Considerations

#### For Existing FraiseQL Users

Include migration section in main documentation:

```markdown
## Migrating Existing Mutations

### From Basic JSON Returns

**Before (basic pattern):**
```sql
RETURN jsonb_build_object('success', true, 'data', v_entity);
```

**After (mutation result pattern):**
```sql
RETURN core.log_and_return_mutation(
    input_pk_organization,
    input_user_id,
    'entity_type',
    v_entity_id,
    'INSERT',
    'new',
    ARRAY['field1', 'field2'],
    'Entity created successfully',
    NULL,
    v_entity_data,
    jsonb_build_object('trigger', 'api_create')
);
```

### Compatibility

The mutation result pattern is:
- ✅ **Backward compatible** - Existing functions continue to work
- ✅ **Incrementally adoptable** - Migrate functions one at a time
- ✅ **Non-breaking** - No changes to existing GraphQL schemas required
```

### Documentation Maintenance

#### Regular Updates
- Keep examples in sync with latest FraiseQL features
- Update cross-references when related documentation changes
- Verify all code examples work with current FraiseQL versions
- Update performance benchmarks and recommendations

#### Version Compatibility
- Note which FraiseQL versions support this pattern
- Document any version-specific considerations
- Provide migration paths for version upgrades
