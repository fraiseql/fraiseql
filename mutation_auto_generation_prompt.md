# Mutation Auto-Generation from Type Definitions

## Context
FraiseQL currently requires explicit mutation class definitions with `input`, `success`, and `error` type annotations:

```python
@fraise_input
class CreateUserInput:
    name: str
    email: str

@fraise_success
class CreateUserSuccess:
    user: User
    message: str

@fraise_failure
class CreateUserError:
    message: str
    code: str

@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError
```

This creates significant boilerplate for simple CRUD operations.

## Current Architecture Analysis

### Mutation Decorator Flow
1. `MutationDefinition` extracts types from class annotations via `get_type_hints()`
2. Creates resolver that calls PostgreSQL function with snake_case name
3. Registers with GraphQL schema

### Type System Patterns
- `WhereInput` and `OrderBy` are auto-generated via lazy properties
- `@fraise_type` decorator adds `LazyWhereInputProperty()` and `LazyOrderByProperty()`
- Access via `SomeType.WhereInput` triggers generation

## Proposed Simplification

### Goal
Automatically generate mutations from type triplets following naming conventions, eliminating the need for explicit mutation classes.

### Potential Approaches

**Option A: Lazy Property on Input Types**
```python
# Add to @fraise_input decorator
if not hasattr(cls, "Mutation"):
    cls.Mutation = LazyMutationProperty()

# Usage
CreateUserInput.Mutation  # Returns configured mutation class
```

**Option B: Registry-Based Auto-Detection**
```python
# During schema building, scan for *Input/*Success/*Error patterns
# Auto-register mutations based on naming conventions
```

**Option C: Explicit Generation Method**
```python
# Auto-detects CreateUserSuccess and CreateUserError
mutation = CreateUserInput.as_mutation()
```

## Exploration Requirements

### 1. Feasibility Analysis
- Can we reliably detect type triplets using naming conventions?
- What are the edge cases and failure modes?
- How does this interact with existing explicit mutations?

### 2. Implementation Strategy
- Where should the auto-generation logic live?
- How to handle PostgreSQL function naming (snake_case conversion)?
- How to support configuration (custom function names, schemas, context params)?

### 3. Type System Integration
- Should this extend the existing lazy property pattern?
- How to integrate with `@fraise_input`, `@fraise_success`, `@fraise_failure` decorators?
- Backward compatibility requirements?

### 4. Developer Experience
- How do developers discover available auto-generated mutations?
- What configuration options should be available?
- How to handle complex mutations that need custom logic?

### 5. Schema Registration
- When should auto-generated mutations be registered?
- How to prevent duplicate registrations?
- Integration with existing schema building pipeline?

## Success Criteria
- Simple CRUD mutations require zero boilerplate
- Type-driven development workflow
- Full backward compatibility
- Clear developer discoverability
- Robust error handling for edge cases

Please analyze the current codebase, identify the best implementation approach, and provide a detailed technical specification for this feature. Include code examples, integration points, and potential challenges.
