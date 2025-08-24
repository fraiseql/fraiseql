# Blog E2E Test Suite - Database-First Architecture Demo

This test suite demonstrates a complete blog application built using database-first architecture patterns inspired by PrintOptim Backend. The focus is on testing error returns and the FraiseQL mutation system with comprehensive error handling.

## Architecture Overview

### The Three-Layer Pattern
```
┌─────────────────────────────────────────────────────────┐
│ GraphQL Layer (FraiseQL)                                │
│ - Sees ONLY public schema (v_* views/tv_* materialized tables) │
│ - ID transformation: pk_[entity] → id                   │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│ PostgreSQL Functions (app.* schema)                     │
│ - Business logic, validations, cache management         │
│ - Returns JSONB with success/error structure            │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│ Command Side (blog schema)                              │
│ - tb_* tables with JSONB data column                    │
│ - UUID primary keys (pk_[entity])                       │
└─────────────────────────────────────────────────────────┘
```

### Domain Models

**Blog Posts**: The core content entity
- Rich text content with metadata
- Author attribution
- Publication state management
- Tag associations

**Authors**: Content creators
- Profile information
- Authentication details
- Post authorship tracking

**Tags**: Content categorization
- Hierarchical tag system
- Usage counting
- Slug-based URLs

**Comments**: User interactions
- Threaded discussions
- Moderation workflow
- Spam detection

## Test Phases - Micro TDD Approach

### Phase 1: RED - Failing Tests
- Create E2E test for blog post creation with comprehensive error scenarios
- Test validation failures, duplicate detection, missing references
- Verify error structure matches FraiseQL expectations

### Phase 2: GREEN - Minimal Implementation
- Implement PostgreSQL schema with tb_*, tv_*, v_* tables
- Create app.* and core.* functions following mutation patterns
- Build FraiseQL GraphQL mutations with PrintOptimMutation pattern

### Phase 3: REFACTOR - Comprehensive Error Handling
- Expand error scenarios and edge cases
- Add performance optimizations
- Implement cache invalidation patterns

## Error Testing Focus

This suite specifically tests the FraiseQL error handling system:

- **NOOP Patterns**: Duplicate detection, validation failures, missing references
- **Error Array Population**: Using DEFAULT_ERROR_CONFIG for structured error responses  
- **Status Codes**: Testing various noop:* status patterns
- **Metadata Handling**: Rich error context and debugging information
- **Cross-Domain Validation**: Testing entity relationships and constraints

## Key Features Demonstrated

- **Database-First Design**: Schema defines API surface
- **Two-Function Pattern**: app.* wrappers → core.* business logic
- **Rich Error Responses**: Comprehensive error metadata and debugging
- **UUID Conventions**: pk_[entity] in command side, id in query side
- **Cache Invalidation**: Using tv_* tables for materialized projections
- **JSONB Data Storage**: Flexible document storage with typed overlays