# Core Module Structure

**Location**: `src/fraiseql/core/`
**Purpose**: GraphQL execution pipeline and type system foundation
**Stability**: Core - may have breaking changes in major versions
**Test Coverage**: 150+ unit tests in `tests/unit/core/`

## Overview

The `core` module contains the fundamental GraphQL execution pipeline. It coordinates parsing, validation, type resolution, and field execution.

## Module Organization

### `graphql_pipeline.py` (Main Entry Point)
**Size**: Large (~15-20KB expected)
**Responsibility**: Coordinates GraphQL request execution
**Public API**:
- `GraphQLPipeline`: Main execution coordinator
- `execute_query()`: Execute GraphQL queries

**Key Methods**:
- `parse()`: Parse GraphQL query string
- `validate()`: Validate against schema
- `execute()`: Execute query and return results

**Depends On**: Registry, AST parser, type system
**Used By**: HTTP servers (FastAPI, Axum, etc)

**When to Modify**:
- Changing query execution flow
- Adding execution phases
- Modifying error handling

---

### `graphql_type.py` (Type System)
**Size**: Large (~45KB - **candidate for refactoring**)
**Responsibility**: Type definition and resolution
**Public API**:
- `GraphQLType`: Base type class
- `@fraise_type`: Type decorator
- Type resolution functions

**Sub-components** (refactoring candidates):
```
graphql_type/
├── type_definition.py      # Type class definition
├── type_registry.py        # Registry management
├── type_validation.py      # Type validation
└── type_conversion.py      # Type coercion
```

**Depends On**: Scalars, type system, validators
**Used By**: Pipeline, decorators, HTTP servers

**When to Modify**:
- Adding new type features
- Changing type resolution
- Modifying type validation

**⚠️ Note**: This file exceeds recommended 1,500-line limit and is a refactoring candidate for v2.1

---

### `registry.py`
**Size**: Small-Medium (~3-5KB)
**Responsibility**: Type registry management
**Public API**:
- `TypeRegistry`: Central type registry
- `register_type()`: Add type to registry
- `get_type()`: Retrieve type

**Depends On**: Type definitions
**Used By**: Pipeline, HTTP servers
**Thread-safe**: Yes (singleton pattern)

**When to Modify**:
- Changing type registration logic
- Adding registry hooks
- Modifying type lookup

---

### `selection_tree.py`
**Size**: Medium (~5-8KB)
**Responsibility**: Query field selection analysis
**Public API**:
- `SelectionTree`: Field selection tree
- `build_selection_tree()`: Build from GraphQL selection set

**Depends On**: AST parser, type system
**Used By**: Pipeline, field resolution

**When to Modify**:
- Changing field selection
- Adding selection analysis
- Optimizing field traversal

---

### `ast_parser.py`
**Size**: Small-Medium (~4-6KB)
**Responsibility**: GraphQL query parsing
**Public API**:
- `parse_query()`: Parse query string
- `parse_mutation()`: Parse mutation
- `parse_subscription()`: Parse subscription

**Depends On**: GraphQL library
**Used By**: Pipeline
**Note**: Delegates to graphql-core library

**When to Modify**:
- Adding custom parsing
- Implementing query extensions
- Modifying parser behavior

---

### `fragment_resolver.py`
**Size**: Small-Medium (~3-5KB)
**Responsibility**: Resolve GraphQL fragments (@include, @skip)
**Public API**:
- `resolve_fragments()`: Process fragment directives
- `FragmentResolver`: Main resolver

**Depends On**: AST parser, selection tree
**Used By**: Pipeline, selection tree building

**When to Modify**:
- Adding fragment directives
- Changing fragment behavior
- Optimizing fragment resolution

---

### `nested_field_resolver.py`
**Size**: Medium (~6-8KB)
**Responsibility**: Resolve deeply nested field selections
**Public API**:
- `resolve_nested_fields()`: Process nested selections
- `NestedFieldResolver`: Main resolver

**Depends On**: Selection tree, type system
**Used By**: Pipeline, SQL generation

**When to Modify**:
- Changing nested field handling
- Adding nested field optimizations
- Modifying field traversal

---

### `rust_pipeline.py`
**Size**: Small (~2-3KB)
**Responsibility**: Integration layer with Rust extension
**Public API**:
- `RustPipeline`: Rust pipeline wrapper
- `execute_rust_query()`: Execute via Rust

**Depends On**: Rust extension (fraiseql_rs)
**Used By**: GraphQL pipeline
**Fallback**: Gracefully degrades if Rust not available

**When to Modify**:
- Adding Rust features
- Changing Rust integration
- Modifying performance optimization

---

## Dependencies

### Internal Dependencies
```
graphql_pipeline.py
  ├── registry.py
  ├── ast_parser.py
  ├── selection_tree.py
  ├── fragment_resolver.py
  ├── nested_field_resolver.py
  ├── graphql_type.py
  └── rust_pipeline.py

graphql_type.py
  ├── types/ (scalars, type definitions)
  ├── registry.py
  └── decorators

selection_tree.py
  ├── ast_parser.py
  ├── graphql_type.py
  └── fragment_resolver.py
```

### External Dependencies
- `graphql-core`: GraphQL parsing and validation
- `pydantic`: Type validation
- `fraiseql_rs`: Performance extension (optional)

---

## Adding New Code to Core

### Guidelines

1. **Keep modules focused**: One responsibility per file
2. **Export clearly**: Use `__init__.py` to define public API
3. **Add type hints**: All public functions must have type hints
4. **Document purpose**: Module docstring explains responsibility
5. **Write tests**: Each new function needs tests

### Template

```python
"""Module description.

This module handles [responsibility].

Example:
    >>> from fraiseql.core import my_function
    >>> result = my_function()

Functions:
    my_function: Main function
"""

from typing import Optional

def my_function(param: str) -> Optional[int]:
    """Function description.

    Args:
        param: Parameter description

    Returns:
        Return value description
    """
    ...
```

### Testing

- Place tests in `tests/unit/core/test_[module].py`
- Test at least happy path + error cases
- Use fixtures from `tests/fixtures/`
- Mark with `@pytest.mark.unit` and `@pytest.mark.core`

---

## Refactoring Roadmap

### v2.0 (Current)
- ✅ Document current structure
- ✅ Establish size guidelines
- ⏳ Monitor file sizes

### v2.1 (Next Minor)
- 📋 Evaluate graphql_type.py refactoring
- 📋 Consider subpackage restructuring
- 📋 Add performance profiling

### v2.2+
- 📋 Execute graphql_type.py refactoring if needed
- 📋 Optimize hot paths

---

## Common Questions

**Q: Where do I add a new resolver?**
A: Add to `graphql_type.py` as a method on the type class or create `resolver_[name].py` if complex.

**Q: How do I trace query execution?**
A: Use pipeline hooks in `graphql_pipeline.py` or enable logging in your HTTP server.

**Q: Can I modify the parsing behavior?**
A: Yes, modify `ast_parser.py` or create a custom parser by subclassing.

**Q: Where do schema caching hooks go?**
A: Add to `registry.py` or create `caching.py` module if complex.

---

## See Also

- **Main documentation**: `docs/ORGANIZATION.md`
- **Related tests**: `tests/unit/core/`
- **Type system**: `src/fraiseql/types/`
- **HTTP servers**: `src/fraiseql/fastapi/`, `src/fraiseql/axum/`

---

**Last Updated**: January 8, 2026
**Stability**: Core
**Next Review**: v2.1 release
