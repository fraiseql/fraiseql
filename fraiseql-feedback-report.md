# FraiseQL Integration Report - pgGit Demo Experience

## Executive Summary

While attempting to create a demo application (pgGit - Git for PostgreSQL) using FraiseQL 0.1.0a4, we encountered several challenges that prevented successful integration. This report documents the issues faced and provides constructive feedback for improving the developer experience.

## Context

- **Project**: pgGit Demo - A concept demonstration of Git-like version control for PostgreSQL
- **Goal**: Showcase both pgGit concept and FraiseQL framework capabilities
- **FraiseQL Version**: 0.1.0a4 (installed from PyPI)
- **Environment**: Python 3.11, FastAPI, Ubuntu server

## Issues Encountered

### 1. Missing API Documentation

**Problem**: The `fraiseql.build_schema()` function referenced in examples does not exist in the installed package.

```python
# This failed:
schema = fraiseql.build_schema(queries=[commits])
# AttributeError: module 'fraiseql' has no attribute 'build_schema'
```

**Impact**: Unable to determine the correct API for creating a GraphQL schema with FraiseQL.

### 2. Unclear Integration Pattern

**Problem**: No clear documentation on how to:
- Initialize FraiseQL with FastAPI
- Create GraphQL types using FraiseQL decorators
- Set up the GraphQL endpoint
- Enable the GraphQL Playground

**What we tried**:
```python
import fraiseql

@fraiseql.type  # Does this decorator exist?
class Commit:
    hash: str
    message: str

@fraiseql.query  # How to register queries?
async def commits() -> List[Commit]:
    pass
```

### 3. GraphQL Playground Integration

**Problem**: Unclear how FraiseQL provides GraphQL Playground functionality:
- Is it built-in?
- Does it require additional configuration?
- What's the default path?

**Expected**: Based on the source code analysis, playground should be available at `/playground` in development mode, but the integration method was unclear.

### 4. Missing Examples

**Problem**: No working examples found for:
- Basic "Hello World" with FraiseQL
- FastAPI integration
- Type definitions and resolvers
- Mutation handling

## What We Expected

Based on the FraiseQL concept (GraphQL-to-PostgreSQL), we expected:

```python
# Expected API pattern:
from fraiseql import create_app, type, query, mutation

@type
class Commit:
    hash: str
    message: str
    author: str

@query
async def get_commits() -> List[Commit]:
    # Automatically translated to PostgreSQL query
    pass

app = create_app(
    database_url="postgresql://...",
    types=[Commit],
    queries=[get_commits]
)
# GraphQL endpoint automatically available
# Playground automatically available at /playground
```

## Recommendations

### 1. Quick Start Documentation

Create a minimal working example:
```python
# examples/quickstart.py
from fraiseql import FraiseQL
from fastapi import FastAPI

app = FastAPI()
fraiseql = FraiseQL(app, database_url="...")

# Show how to add types, queries, mutations
```

### 2. API Reference

Document all public APIs:
- Decorators: `@type`, `@query`, `@mutation`, `@input`
- Functions: How to create the app, configure playground
- Classes: Main FraiseQL class and its methods

### 3. Integration Guides

Provide guides for:
- FastAPI integration
- Standalone usage
- PostgreSQL connection setup
- Playground configuration

### 4. Error Messages

Improve error messages to guide users:
```python
# Instead of: AttributeError: module 'fraiseql' has no attribute 'build_schema'
# Better: "FraiseQL: build_schema not found. Use fraiseql.create_app() instead. See docs: ..."
```

## Positive Observations

Despite the challenges, we appreciate:

1. **The Concept**: GraphQL-to-PostgreSQL translation is innovative and valuable
2. **Type Safety**: The decorator-based approach for type definitions is elegant
3. **CQRS Pattern**: Using views for queries and functions for mutations is clever
4. **Alpha Status**: We understand this is alpha software and expect some rough edges

## Conclusion

FraiseQL has great potential, but the current alpha version lacks the documentation and examples needed for successful integration. We ended up creating a traditional GraphQL endpoint instead of leveraging FraiseQL's capabilities.

We hope this feedback helps improve the developer experience for future users. The pgGit demo would have been an excellent showcase for FraiseQL's capabilities if we could have successfully integrated it.

## Test Code Available

We're happy to share our attempted integration code if it would help identify where documentation could be improved.

---

*Report prepared for the FraiseQL development team*  
*Date: June 17, 2025*  
*Context: Attempting to create a demo for Hacker News launch*
