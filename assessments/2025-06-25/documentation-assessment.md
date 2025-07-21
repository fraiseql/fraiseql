# FraiseQL Documentation Assessment

## Executive Summary

After reviewing the FraiseQL documentation in light of the PrintOptim team's issues, several clarity gaps have been identified that contribute to user confusion.

## Critical Documentation Gaps

### 1. Query Pattern Documentation ❌

**Current State**: The README shows a basic example but doesn't explain FraiseQL's query patterns clearly.

**Problems**:
- No clear explanation that `@fraiseql.query` is the primary pattern
- No mention that `resolve_` prefix is NOT used
- Mixed examples without explaining when to use each pattern
- No clear "info" parameter documentation

**Impact**: Users try traditional GraphQL patterns and get `None` info parameters.

### 2. Database Connection Pattern ❌

**Current State**: Examples show direct database access but don't explain the FraiseQLRepository pattern.

**Problems**:
- No documentation about FraiseQLRepository being required
- No explanation of connection lifecycle management
- Missing context_getter pattern documentation
- No multi-tenant pattern examples

**Impact**: Users try to pass raw connections and get connection lifecycle errors.

### 3. JSONB Data Column Pattern ⚠️

**Current State**: Updated in README but not prominent enough.

**Problems**:
- Breaking change notice is buried in installation section
- No clear "Architecture" section explaining the pattern
- Migration guide exists but isn't linked prominently
- Examples still show old patterns in some places

**Impact**: Users don't understand why views must have a 'data' column.

### 4. Context Access Pattern ❌

**Current State**: Scattered mentions without consolidated explanation.

**Problems**:
- No clear documentation on what's in `info.context`
- No explanation of custom context_getter
- No examples of accessing tenant_id, user, etc.
- Missing explanation of built-in vs custom context

**Impact**: Users don't know how to access database, auth, or custom values.

## Documentation Clarity Assessment

### README.md - Score: 6/10

**Strengths**:
- Good project overview
- Clear installation instructions
- Nice feature list

**Weaknesses**:
- Jumps into complex examples too quickly
- Mixes multiple patterns without explanation
- No clear "Getting Started" path
- JSONB pattern explanation comes too late

### Quick Start Guide - Score: 5/10

**Strengths**:
- Shows basic usage
- Has troubleshooting section

**Weaknesses**:
- Still uses old database patterns in places
- Doesn't explain query patterns clearly
- No mention of FraiseQLRepository
- Lacks progression from simple to complex

### API Reference - Score: 3/10

**Critical Gap**: No comprehensive API reference exists!
- No documentation of decorators
- No documentation of FraiseQLRepository methods
- No documentation of context structure
- No type reference

## Specific Issues from PrintOptim

1. **"resolve_machines" vs "machines"**: Not documented anywhere
2. **Info parameter None**: No explanation of parameter order
3. **Database connection handling**: No clear pattern documentation
4. **JSONB data column**: Breaking change not prominent enough
5. **Two-tier resolver pattern**: No mention this is wrong

## Recommendations

### 1. Create a Clear Getting Started Flow

```markdown
# Getting Started with FraiseQL

## 1. Basic Concepts
- Types are Python classes with @fraise_type
- Queries are functions with @fraiseql.query
- All data comes from JSONB 'data' columns
- FraiseQLRepository handles database access

## 2. Your First Query
[Simple example with explanation]

## 3. Database Integration
[Show FraiseQLRepository pattern]

## 4. Adding Context
[Show context_getter pattern]
```

### 2. Add Architecture Documentation

```markdown
# FraiseQL Architecture

## Core Patterns
1. JSONB Data Column Pattern
2. Query Resolution Pattern
3. Context Management Pattern
4. Repository Pattern

[Detailed explanation of each]
```

### 3. Create Comprehensive API Reference

```markdown
# API Reference

## Decorators
- @fraiseql.type
- @fraiseql.query
- @fraiseql.mutation
- @fraiseql.field

## Repository
- FraiseQLRepository
  - find()
  - find_one()
  - run()

## Context
- Default context structure
- Custom context_getter
```

### 4. Add Common Patterns Guide

```markdown
# Common Patterns

## Multi-Tenant Applications
[Complete example]

## Authentication
[Complete example]

## Custom Context
[Complete example]
```

### 5. Improve Error Messages

Instead of:
```
'NoneType' object has no attribute 'context'
```

Should be:
```
Query function 'machines' has invalid signature.
Expected: @fraiseql.query decorated function with (info, ...) parameters.
Got: method with 'resolve_' prefix (not supported in FraiseQL).
See: https://docs.fraiseql.com/query-patterns
```

## Conclusion

The documentation needs significant improvement in:
1. **Clear patterns**: Explain FraiseQL's opinionated approach upfront
2. **Complete examples**: Show full working code, not fragments
3. **API reference**: Document all public APIs
4. **Progressive learning**: Start simple, build complexity
5. **Better errors**: Guide users to solutions

The PrintOptim team's struggles are directly related to these documentation gaps. Their issues would be prevented with clearer documentation of FraiseQL's patterns.
