# Phase 1: Clarify Auto-Injection Scope Documentation

**Objective**: Update documentation to clearly explain what auto-injection does and doesn't do

**Priority**: P0 - URGENT (Critical documentation gap)
**Estimated Time**: 2 hours
**Dependencies**: None
**Phase Type**: Documentation Update

---

## Context

**Current State**:
- Documentation doesn't clearly state what auto-injection covers
- Users (like PrintOptim) misunderstand scope and create empty base classes
- No clear examples of when to use auto-injection vs explicit fields

**User Impact**:
- Teams waste time implementing incorrect patterns
- Support requests about "missing fields" warnings
- Confusion about decorator behavior

**Target State**:
- Clear documentation of what `@fraiseql.success` and `@fraiseql.failure` inject
- Explicit list of what they DON'T inject
- Examples showing when auto-injection applies vs when it doesn't

---

## Files to Create/Modify

### Primary Documentation File

**Location**: `docs/mutations/auto-injection.md` (create new)

**Or if using existing structure**: Add section to existing mutations guide

**Alternative Locations**:
- `docs/guides/mutations.md` - Add "Auto-Injection" section
- `docs/reference/decorators.md` - Update decorator reference
- README.md - Add to quick start if relevant

---

## Implementation Steps

### Step 1: Create Auto-Injection Documentation Section

**File**: `docs/mutations/auto-injection.md`

**Content**:
```markdown
# Mutation Auto-Injection

FraiseQL's `@fraiseql.success` and `@fraiseql.failure` decorators provide automatic
field injection to reduce boilerplate in mutation response types.

## What Auto-Injection Does

The decorators automatically inject **three standard fields** into mutation response types:

- `status: str` - Operation status ("success", "failed:*", "noop:*")
- `message: str | None` - Human-readable description of the result
- `errors: list[Error] | None` - Structured error array (empty for success)

### When Injection Occurs

Fields are only injected if they are **NOT already defined** in the class:

```python
# Example 1: Auto-injection happens
@fraiseql.success
class CreateUserSuccess:
    user: User
    # ✅ status, message, errors automatically added

# Example 2: Auto-injection skipped (fields already present)
@fraiseql.success
class CreateUserSuccess:
    status: str = "success"
    message: str | None = None
    errors: list[Error] | None = None
    user: User
    # ✅ Nothing injected (fields already defined)
```

## What Auto-Injection Does NOT Do

Auto-injection is limited in scope. It does **NOT** inject:

❌ Entity-specific fields (e.g., `user`, `machine`, `post`)
❌ `id` field (this is a runtime mapping - see below)
❌ `updatedFields` field (this is a runtime mapping - see below)
❌ Cascade data fields
❌ Any custom/domain-specific fields

**Important**: You must explicitly define all entity-specific fields in your types.

### Runtime Mappings vs Auto-Injection

Some fields appear in GraphQL responses but are NOT defined in Python types.
These are **runtime mappings** handled by FraiseQL's Rust layer:

| Database Field | GraphQL Field | Where It's Added |
|----------------|---------------|------------------|
| `entity_id` | `id` | Rust response builder |
| `updated_fields` | `updatedFields` | Rust response builder |

**Do NOT define these in Python types** - they're added to the JSON response at runtime.

## Common Patterns

### Pattern 1: Minimal Types (Leverages Auto-Injection)

Best for small projects with few mutations:

```python
@fraiseql.success
class CreateUserSuccess:
    """status, message, errors auto-injected"""
    user: User
    cascade: Cascade | None = None

@fraiseql.failure
class CreateUserError:
    """status, message, errors auto-injected"""
    conflict_user: User | None = None
```

**Pros**: Less code, DRY
**Cons**: Implicit behavior (less discoverable)

### Pattern 2: Explicit Base Class (Recommended for Large Projects)

Best for large projects with many mutations:

```python
from fraiseql.types.errors import Error

class MutationResultBase:
    """Explicit base class for all mutations."""
    status: str = "success"
    message: str | None = None
    errors: list[Error] | None = None

@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    """Inherits status, message, errors from base"""
    user: User
    cascade: Cascade | None = None

@fraiseql.failure
class CreateUserError(MutationResultBase):
    """Inherits status, message, errors from base"""
    conflict_user: User | None = None
```

**Pros**: Explicit, self-documenting, IDE-friendly
**Cons**: Slightly more code

### Pattern 3: FraiseQL's Built-in Base

Use FraiseQL's provided base class:

```python
from fraiseql.types.common import MutationResultBase

@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    user: User
```

**Pros**: Standard, maintained by FraiseQL
**Cons**: External dependency

## Anti-Patterns

### ❌ Empty Base Class

**Don't do this**:
```python
class MutationResultBase:
    pass  # Empty - provides NOTHING!

@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    user: User
```

**Problem**: Empty base class provides no fields to child classes. If you want a
base class, it must define the common fields.

**Fix**: Either define fields in the base class or remove it entirely.

### ❌ Defining Runtime-Mapped Fields

**Don't do this**:
```python
@fraiseql.success
class CreateUserSuccess:
    id: UUID  # ❌ Runtime-mapped, don't define
    updatedFields: list[str]  # ❌ Runtime-mapped, don't define
    user: User
```

**Problem**: These fields are added by the Rust response builder at runtime,
not by Python type definitions.

**Fix**: Remove these fields. They'll appear in responses automatically.

## Debugging Auto-Injection

If you see schema validation warnings about missing fields:

1. **Check for empty base class**: Ensure base class defines fields or remove it
2. **Verify decorator usage**: Use `@fraiseql.success` or `@fraiseql.failure`
3. **Check field names**: Ensure you're defining `status`, not `Status`
4. **Inspect annotations**: Print `YourClass.__annotations__` to see what's defined

## See Also

- [Runtime Field Mapping](./runtime-mapping.md) - How `entity_id` becomes `id`
- [Mutation Patterns](./patterns.md) - Complete mutation examples
- [Error Handling](./errors.md) - Working with structured errors
```

---

### Step 2: Add Visual Decision Tree

**File**: `docs/mutations/auto-injection-decision-tree.md`

**Content**:
```markdown
# Auto-Injection Decision Tree

## Should I define status/message/errors fields?

```
                    START
                      |
                      v
        Do you want explicit control
        over default values?
                  /     \
              YES        NO
               |          |
               v          v
        Define fields  Let decorator
        explicitly     auto-inject
               |          |
               |          v
               |      Define only
               |      entity fields
               |          |
               v          v
        Use base class OR define in each type
               |
               v
            DONE
```

## Should I use a base class?

```
                    START
                      |
                      v
        How many mutations do you have?
                  /          \
            < 10              > 10
              |                 |
              v                 v
        Small project     Large project
        Base class        Base class
        OPTIONAL          RECOMMENDED
              |                 |
              v                 v
        Pattern 1 or 2    Pattern 2
```

## Should I define `id` or `updatedFields`?

```
                    START
                      |
                      v
        Is this field mapped from
        database at runtime?
                  /     \
              YES        NO
               |          |
               v          v
        DON'T define  Define it
        (Rust maps    in Python
         it for you)   type
               |          |
               v          v
            DONE        DONE
```

**Runtime-Mapped Fields** (DON'T define):
- `id` (from `entity_id`)
- `updatedFields` (from `updated_fields`)

**Entity Fields** (DO define):
- `user`, `machine`, `post`, etc.
- `cascade`
- Any domain-specific fields
```

---

### Step 3: Update Decorator Reference

**File**: `docs/reference/decorators.md` (or create if doesn't exist)

**Add Section**:
```markdown
## @fraiseql.success

Marks a class as a successful mutation response type.

**Auto-Injection**: Automatically adds `status`, `message`, and `errors` fields
if they are not already defined in the class.

**Usage**:
```python
@fraiseql.success
class CreateUserSuccess:
    user: User
```

**Generated Schema**:
```graphql
type CreateUserSuccess {
  status: String!
  message: String
  errors: [Error]
  user: User!
}
```

**Parameters**: None

**See Also**: [Auto-Injection Guide](../mutations/auto-injection.md)

---

## @fraiseql.failure

Marks a class as an error mutation response type.

**Auto-Injection**: Automatically adds `status`, `message`, and `errors` fields
if they are not already defined in the class.

**Usage**:
```python
@fraiseql.failure
class CreateUserError:
    conflict_user: User | None = None
```

**Generated Schema**:
```graphql
type CreateUserError {
  status: String!
  message: String
  errors: [Error]
  conflictUser: User
}
```

**Parameters**: None

**Note**: Use `@fraiseql.failure`, not `@fraiseql.error`

**See Also**: [Auto-Injection Guide](../mutations/auto-injection.md)
```

---

### Step 4: Add FAQ Entries

**File**: `docs/faq.md` (add to existing or create)

**Add Questions**:
```markdown
## Mutation Auto-Injection

### Q: Do I need to define status/message/errors fields in my mutation types?

**A**: No, if you want to use auto-injection. The `@fraiseql.success` and
`@fraiseql.failure` decorators automatically add these fields if they're not
already present.

However, for large projects, explicitly defining them in a base class is
recommended for clarity and IDE support.

**Example**:
```python
# Auto-injection (minimal)
@fraiseql.success
class CreateUserSuccess:
    user: User

# Explicit (recommended for large projects)
class MutationResultBase:
    status: str = "success"
    message: str | None = None
    errors: list[Error] | None = None

@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    user: User
```

---

### Q: Why do I see "Missing expected fields" warnings?

**A**: This usually means:

1. **Empty base class**: You're inheriting from a base class that doesn't define fields
2. **Wrong decorator**: Using incorrect decorator name
3. **Field name mismatch**: Using `Status` instead of `status`, etc.

**Fix**: Either define fields in your base class or remove the base class entirely.

❌ **Don't do this**:
```python
class MutationResultBase:
    pass  # Empty!

@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    user: User
```

✅ **Do this instead**:
```python
# Option 1: No base class
@fraiseql.success
class CreateUserSuccess:
    user: User

# Option 2: Base class with fields
class MutationResultBase:
    status: str = "success"
    message: str | None = None
    errors: list[Error] | None = None

@fraiseql.success
class CreateUserSuccess(MutationResultBase):
    user: User
```

---

### Q: Should I define `id` and `updatedFields` in my Python types?

**A**: **No!** These fields are runtime mappings from database fields:

- `id` is mapped from `entity_id` by FraiseQL's Rust response builder
- `updatedFields` is mapped from `updated_fields` (snake_case → camelCase)

These transformations happen in the **GraphQL response JSON**, not in your
Python type definitions.

**Example**:
```python
# ✅ CORRECT: Don't define runtime-mapped fields
@fraiseql.success
class CreateUserSuccess:
    user: User
    # id and updatedFields will appear in response automatically

# ❌ WRONG: Don't do this
@fraiseql.success
class CreateUserSuccess:
    id: UUID  # ← Runtime-mapped, don't define!
    updatedFields: list[str]  # ← Runtime-mapped, don't define!
    user: User
```

**GraphQL Response** (automatically includes `id` and `updatedFields`):
```json
{
  "createUser": {
    "status": "success",
    "message": "User created",
    "id": "123e4567...",
    "updatedFields": ["email", "name"],
    "user": { ... }
  }
}
```

**See**: [Runtime Field Mapping](./mutations/runtime-mapping.md)

---

### Q: When should I use a base class for mutations?

**A**:

**Use base class when**:
- Large project (10+ mutations)
- Want explicit field definitions
- Need IDE autocomplete
- Team prefers explicit over implicit

**Skip base class when**:
- Small project (< 10 mutations)
- Want minimal code
- Comfortable with decorator magic

**Both approaches are valid!** Choose based on project size and team preference.
```

---

## Verification Steps

### Verification 1: Documentation Build

**Command** (adjust for your docs system):
```bash
# If using MkDocs
mkdocs build --strict

# If using Sphinx
sphinx-build -W docs build

# If using custom system
./build-docs.sh
```

**Expected Output**: Docs build without warnings

---

### Verification 2: Link Check

**Command**:
```bash
# Check for broken internal links
find docs -name "*.md" -exec grep -H "\[.*\](.*)" {} \; | \
  grep -v "http" | \
  # Extract file paths and verify they exist
  # (implementation depends on your link format)
```

**Expected Output**: No broken links

---

### Verification 3: Content Review Checklist

**Manual Review**:
- [ ] Auto-injection scope is clearly explained
- [ ] "What it does" section is complete
- [ ] "What it does NOT do" section is clear
- [ ] All three patterns are documented
- [ ] Anti-patterns are clearly marked
- [ ] Code examples are syntactically correct
- [ ] GraphQL schema examples match Python code
- [ ] Links to related documentation work

---

### Verification 4: User Perspective Test

**Ask a colleague** (who doesn't know the issue) to:
1. Read the new documentation
2. Answer these questions:
   - "Should I define status/message/errors fields?"
   - "Should I define id and updatedFields fields?"
   - "What's wrong with an empty base class?"

**Expected**: They answer all correctly based on docs

---

## Acceptance Criteria

**Must Have**:
- [ ] New auto-injection documentation page created
- [ ] Clear explanation of what auto-injection does and doesn't do
- [ ] At least 3 common patterns documented
- [ ] Anti-patterns clearly marked
- [ ] FAQ entries added
- [ ] Decorator reference updated
- [ ] All code examples are syntactically correct
- [ ] Documentation builds without errors

**Success Metrics**:
- Users can answer "should I define X field?" correctly
- No ambiguity about runtime mappings vs Python fields
- Empty base class anti-pattern is clearly explained

**Nice to Have**:
- Visual decision tree
- Video tutorial
- Migration guide from old patterns

---

## Rollback Plan

**If Issues Found**:

1. **Documentation errors**: Fix and re-deploy
2. **User confusion**: Add clarifying examples
3. **Broken links**: Update links

**No code changes** in this phase - pure documentation

---

## DO NOT

❌ **DO NOT** change any FraiseQL framework code (that's later phases)
❌ **DO NOT** remove existing documentation (only add/enhance)
❌ **DO NOT** change decorator behavior (document existing behavior)
❌ **DO NOT** deprecate any features
❌ **DO NOT** add features (just document what exists)

---

## Next Phase

After this phase:
- **Phase 2**: Document runtime field mapping in detail
- **Phase 3**: Provide complete reference implementation
- **Phase 4**: Document base class patterns

---

## Notes

**Why This Matters**:
- PrintOptim's confusion shows documentation gap
- Likely affects other users too
- Clear docs prevent support requests
- Helps users get started correctly

**Key Message**:
"Auto-injection is limited to status/message/errors. Everything else you must define explicitly."

**Emphasis**:
- Runtime mappings ≠ auto-injection
- Empty base class = anti-pattern
- Both explicit and auto-injection patterns are valid

---

**Phase Owner**: FraiseQL Documentation Team
**Reviewer**: Framework Maintainer + Tech Writer
**Estimated Completion**: 2 hours
**Status**: Ready for Implementation
