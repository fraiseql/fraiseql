# Blog E2E Test Suite - Complete Implementation Guide

This comprehensive guide documents the complete E2E test suite that demonstrates database-first architecture patterns using FraiseQL and PostgreSQL, with a focus on error handling and comprehensive testing strategies.

## 🎯 Project Overview

This test suite serves as a **reference implementation** for:
- Database-first GraphQL API development
- Comprehensive error handling with FraiseQL
- Micro TDD approach (RED → GREEN → REFACTOR)
- PrintOptim Backend architecture patterns
- PostgreSQL as single source of truth

## 📁 File Structure

```
tests/blog_e2e/
├── README.md                     # Project overview and architecture
├── IMPLEMENTATION_GUIDE.md       # This comprehensive guide
├── schema.sql                    # Database schema (command/query separation)
├── functions.sql                 # PostgreSQL functions (app.* → core.*)
├── conftest.py                   # Test fixtures and database setup
├── graphql_types.py              # FraiseQL types and mutations
├── test_red_phase.py            # RED phase - failing tests
├── test_refactor_phase.py       # REFACTOR phase - comprehensive tests
├── run_red_phase.py             # RED phase runner
├── run_green_phase.py           # GREEN phase runner
└── run_refactor_phase.py        # REFACTOR phase runner
```

## 🏗️ Architecture Deep Dive

### The Three-Layer Pattern

Our architecture follows PrintOptim Backend's three-layer separation:

```
┌─────────────────────────────────────────────────────────┐
│ GraphQL Layer (FraiseQL)                                │
│ - Input validation and type conversion                  │
│ - Error response mapping                                │
│ - ID transformation: pk_[entity] → id                   │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│ PostgreSQL Functions (app.* → core.*)                   │
│ - app.*: JSONB input handling and delegation            │
│ - core.*: Business logic and validation                 │
│ - Returns structured app.mutation_result                │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│ Command/Query Separation                                │
│ - tb_*: Command side (source of truth)                 │
│ - tv_*: Query side (materialized projections)          │
│ - v_*: Real-time views for immediate consistency        │
└─────────────────────────────────────────────────────────┘
```

### Database Schema Design

#### Command Side Tables (tb_*)
- **tb_author**: Content creators with JSONB profile data
- **tb_post**: Blog posts with flexible content structure
- **tb_tag**: Hierarchical content categorization
- **tb_comment**: Threaded discussions with moderation
- **tb_post_tag**: Many-to-many associations

#### Query Side Tables (tv_*)
- **tv_author**: Denormalized authors with post statistics
- **tv_post**: Complete posts with author and tag data embedded
- **tv_tag**: Tags with hierarchy and usage information
- **tv_comment**: Comments with threading metadata

#### Real-time Views (v_*)
- Live views over command tables for immediate consistency
- Used when materialized tables haven't been refreshed yet

### The Two-Function Pattern

Every mutation follows PrintOptim's two-function pattern:

```sql
-- App wrapper: Handles JSONB input from GraphQL
CREATE FUNCTION app.create_author(
    input_created_by UUID,
    input_payload JSONB  -- Raw from GraphQL
) RETURNS app.mutation_result

-- Core logic: Business validation and operations
CREATE FUNCTION core.create_author(
    input_created_by UUID,
    input_data app.type_author_input,  -- Typed
    input_payload JSONB                -- For logging
) RETURNS app.mutation_result
```

## 🧪 Testing Strategy - Micro TDD

### Phase 1: RED - Failing Tests
**File**: `test_red_phase.py`
**Runner**: `run_red_phase.py`

- Define comprehensive test scenarios **before** implementation
- Test all error conditions and edge cases
- Validate expected GraphQL API surface
- All tests should **fail** initially (no implementation yet)

**Key Test Categories**:
- Success cases (happy path)
- Validation errors (missing fields, invalid formats)
- Business rule violations (duplicate identifiers, missing references)
- Content validation (length limits, security checks)
- Complex relationships (tag hierarchies, author associations)

### Phase 2: GREEN - Minimal Implementation
**Files**: `functions.sql`, `graphql_types.py`
**Runner**: `run_green_phase.py`

- Implement **minimal** code to make RED tests pass
- Focus on core business logic without optimization
- Comprehensive error handling from the start
- Database functions as single source of truth

**Implementation Includes**:
- PostgreSQL functions with validation
- FraiseQL mutations using BlogMutationBase pattern
- Structured error responses with rich metadata
- Basic materialized table refresh patterns

### Phase 3: REFACTOR - Comprehensive Enhancement
**File**: `test_refactor_phase.py`
**Runner**: `run_refactor_phase.py`

- Add advanced test scenarios and optimizations
- Performance testing and bulk operations
- Security validation patterns
- Database transaction integrity
- Cache invalidation verification

**Enhanced Test Categories**:
- Advanced validation (email normalization, slug patterns)
- Security scenarios (content validation, injection prevention)
- Performance characteristics (bulk operations, complex queries)
- Transaction integrity (rollback scenarios)
- Cache consistency (materialized table updates)

## 🔧 Key Implementation Patterns

### Error Handling with NOOP Patterns

Following PrintOptim's NOOP (No Operation) patterns:

```sql
-- Example: Duplicate author detection
IF v_existing_id IS NOT NULL THEN
    RETURN core.log_and_return_mutation(
        'author', v_existing_id, 'NOOP', 'noop:duplicate_identifier',
        ARRAY[]::TEXT[], 'Author already exists',
        v_payload_before, v_payload_before,
        jsonb_build_object(
            'reason', 'duplicate_identifier',
            'conflict_id', v_existing_id,
            'input_payload', input_payload
        )
    );
END IF;
```

**NOOP Status Codes**:
- `noop:duplicate_identifier` - Entity already exists
- `noop:missing_author` - Referenced author not found
- `noop:invalid_status` - Invalid status value
- `noop:content_too_long` - Content exceeds limits
- `noop:invalid_tags` - Referenced tags don't exist

### Rich Error Responses

FraiseQL mutations return comprehensive error information:

```typescript
// GraphQL Error Response Structure
{
  "data": {
    "createPost": {
      "__typename": "CreatePostError",
      "message": "Author with identifier 'missing-author' not found",
      "errorCode": "MISSING_AUTHOR",
      "missingAuthor": {
        "identifier": "missing-author"
      },
      "originalPayload": {
        "title": "Test Post",
        "authorIdentifier": "missing-author",
        // ... full input payload
      }
    }
  }
}
```

### Materialized Table Refresh Pattern

Maintaining consistency between command and query sides:

```sql
-- After successful mutation
PERFORM core.refresh_post(ARRAY[v_id]);     -- Update tv_post
PERFORM core.refresh_author(ARRAY[v_author_id]); -- Update related tv_author

-- Refresh function updates denormalized data
CREATE FUNCTION core.refresh_post(post_ids UUID[])
RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_post WHERE id = ANY(post_ids);

    INSERT INTO tv_post (id, identifier, author_id, data, ...)
    SELECT
        p.pk_post,
        p.identifier,
        p.fk_author,
        jsonb_build_object(
            'title', p.data->>'title',
            'author', jsonb_build_object(
                'id', a.pk_author,
                'name', a.data->>'name'
            ),
            'tags', COALESCE(tag_data.tags, '[]'::jsonb)
        ),
        ...
    FROM blog.tb_post p
    JOIN blog.tb_author a ON p.fk_author = a.pk_author
    -- Complex joins for denormalization
    WHERE p.pk_post = ANY(post_ids);
END;
$$;
```

## 🎯 Running the Test Suite

### Prerequisites

```bash
# Install dependencies
pip install pytest pytest-asyncio asyncpg fraiseql fastapi httpx

# Ensure PostgreSQL is running
sudo systemctl start postgresql  # Linux
brew services start postgresql   # macOS
```

### Phase-by-Phase Execution

#### 1. RED Phase - See Tests Fail
```bash
cd /home/lionel/code/fraiseql/tests/blog_e2e
./run_red_phase.py
```
**Expected**: All tests fail (no implementation yet)

#### 2. GREEN Phase - Minimal Implementation
```bash
./run_green_phase.py
```
**Expected**: All tests pass with basic implementation

#### 3. REFACTOR Phase - Comprehensive Testing
```bash
./run_refactor_phase.py
```
**Expected**: Enhanced tests demonstrate advanced patterns

### Individual Test Execution

```bash
# Run specific test file
pytest test_red_phase.py -v

# Run specific test class
pytest test_red_phase.py::TestBlogPostCreationErrors -v

# Run specific test method
pytest test_red_phase.py::TestBlogPostCreationErrors::test_create_post_missing_author_error -v

# Run with database debug info
pytest test_red_phase.py -v -s --log-cli-level=DEBUG
```

## 📊 Test Coverage and Scenarios

### Author Creation Tests
- ✅ Successful creation with all fields
- ✅ Missing required fields (identifier, name, email)
- ✅ Invalid email format validation
- ✅ Duplicate identifier detection
- ✅ Duplicate email detection
- ✅ Email normalization patterns
- ✅ Identifier slug validation rules

### Post Creation Tests
- ✅ Successful creation with author and tags
- ✅ Missing author reference handling
- ✅ Duplicate identifier prevention
- ✅ Invalid status validation
- ✅ Content length validation (10,000 char limit)
- ✅ Invalid tag references
- ✅ Publish date validation logic
- ✅ Content security validation (XSS, injection)
- ✅ Complex tag hierarchy handling

### Error Metadata Tests
- ✅ Comprehensive error information
- ✅ Consistent error structure across mutations
- ✅ Original payload preservation
- ✅ Conflict entity information
- ✅ Debugging metadata inclusion

### Performance Tests
- ✅ Bulk author creation (50 authors)
- ✅ Complex posts with many tags (20+ tags)
- ✅ Response time validation (< 2s for complex operations)
- ✅ Database query optimization

### Database Integrity Tests
- ✅ Transaction rollback on validation failure
- ✅ No partial data insertion on errors
- ✅ Materialized table consistency
- ✅ Cache invalidation patterns
- ✅ Concurrent operation handling

## 🛠️ Customization and Extension

### Adding New Entities

1. **Update Schema** (`schema.sql`):
   ```sql
   CREATE TABLE blog.tb_new_entity (
       id SERIAL,
       pk_new_entity UUID PRIMARY KEY DEFAULT gen_random_uuid(),
       -- Add fields following patterns
   );

   CREATE TABLE tv_new_entity (
       id UUID PRIMARY KEY,
       -- Denormalized fields
   );
   ```

2. **Add Functions** (`functions.sql`):
   ```sql
   CREATE FUNCTION app.create_new_entity(...) RETURNS app.mutation_result;
   CREATE FUNCTION core.create_new_entity(...) RETURNS app.mutation_result;
   ```

3. **Create GraphQL Types** (`graphql_types.py`):
   ```python
   @fraiseql.input
   class CreateNewEntityInput:
       # Define input fields

   class CreateNewEntity(BlogMutationBase, function="create_new_entity"):
       input: CreateNewEntityInput
       success: CreateNewEntitySuccess
       failure: CreateNewEntityError
   ```

4. **Add Tests**:
   ```python
   class TestNewEntityCreation:
       async def test_create_new_entity_success(self, graphql_client):
           # Test implementation
   ```

### Custom Validation Patterns

```sql
-- Add to core function
IF custom_validation_condition THEN
    RETURN core.log_and_return_mutation(
        'entity', v_id, 'NOOP', 'noop:custom_validation',
        ARRAY[]::TEXT[], 'Custom validation message',
        NULL, NULL,
        jsonb_build_object(
            'reason', 'custom_validation_failure',
            'validation_details', additional_context
        )
    );
END IF;
```

### Performance Optimization

1. **Add Indexes**:
   ```sql
   CREATE INDEX idx_tb_entity_custom_field ON blog.tb_entity(custom_field);
   ```

2. **Optimize Refresh Functions**:
   ```sql
   -- Batch updates instead of individual refreshes
   PERFORM core.refresh_entities_batch(entity_ids);
   ```

3. **Add Caching**:
   ```sql
   -- Implement lazy caching patterns
   CREATE TABLE cache.tb_entity_cache (...);
   ```

## 🔍 Troubleshooting Guide

### Common Issues

#### Database Connection Errors
```bash
# Check PostgreSQL is running
sudo systemctl status postgresql

# Check connection parameters in conftest.py
TEST_DB_CONFIG = {
    "host": "localhost",      # Update if needed
    "port": 5432,            # Update if needed
    "user": "postgres",      # Update if needed
    "password": "postgres",  # Update if needed
}
```

#### Import Errors
```bash
# Ensure all dependencies are installed
pip install -r requirements.txt  # If you create one

# Or install individually
pip install fraiseql pytest pytest-asyncio asyncpg fastapi httpx
```

#### Test Failures

1. **Schema Issues**: Check `schema.sql` loaded correctly
2. **Function Issues**: Check `functions.sql` syntax
3. **Type Issues**: Check `graphql_types.py` imports
4. **Data Issues**: Ensure `clean_database` fixture works

#### Performance Issues
- Increase timeout values in test runners
- Check PostgreSQL configuration (shared_buffers, work_mem)
- Add database indexes for test queries
- Consider running tests against dedicated test database

### Debugging Tips

1. **Enable SQL Logging**:
   ```python
   # In conftest.py
   import logging
   logging.basicConfig(level=logging.DEBUG)
   ```

2. **Check Database State**:
   ```bash
   # Connect to test database
   psql -h localhost -U postgres -d blog_e2e_test

   # Check tables
   \dt blog.*
   \dt  # tv_* tables

   # Check functions
   \df app.*
   \df core.*
   ```

3. **Isolate Test Cases**:
   ```bash
   # Run single test for debugging
   pytest test_red_phase.py::TestBlogPostCreationErrors::test_create_post_success_case -v -s
   ```

## 🎓 Learning Outcomes

After working through this E2E test suite, you will understand:

1. **Database-First Architecture**: How to design APIs from database schema up
2. **Comprehensive Error Handling**: Rich error responses with debugging context
3. **PostgreSQL as Business Logic Layer**: Functions as single source of truth
4. **FraiseQL Patterns**: Advanced GraphQL mutation patterns with error handling
5. **Testing Strategies**: Micro TDD with RED → GREEN → REFACTOR phases
6. **Performance Considerations**: Materialized tables and caching patterns
7. **Transaction Integrity**: ACID properties in complex business operations

## 🚀 Production Considerations

This test suite demonstrates patterns suitable for production:

- **Security**: Content validation and SQL injection prevention
- **Performance**: Materialized tables and indexing strategies
- **Reliability**: Transaction integrity and error recovery
- **Maintainability**: Clear separation of concerns and testability
- **Scalability**: Cacheable projections and efficient queries
- **Observability**: Rich error metadata and audit trails

The patterns shown here scale to production applications with proper:
- Connection pooling and database optimization
- Monitoring and alerting integration
- Security hardening and access controls
- Horizontal scaling and load balancing
- CI/CD integration and deployment automation

---

*This implementation guide serves as a comprehensive reference for database-first GraphQL development with FraiseQL and PostgreSQL.*
