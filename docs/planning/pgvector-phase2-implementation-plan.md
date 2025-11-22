# pgvector Phase 2 Implementation Plan

**Status**: Planning
**Complexity**: Complex | **Phased TDD Approach**

## Executive Summary

This plan extends FraiseQL's pgvector support with:
1. Integration test fix for ORDER BY vector distance (currently skipped)
2. Additional pgvector operators (L1/Manhattan, Hamming, Jaccard)
3. Complete ORDER BY vector distance implementation with proper GraphQL input objects

Current state: 5/6 integration tests passing, vector WHERE filters working, ORDER BY infrastructure exists but integration test skipped.

## Current Implementation Status

### ✅ Completed (Phase 1)
- Vector field detection via name patterns (embedding, vector, etc.)
- VectorFilter GraphQL input with 3 operators:
  - `cosine_distance` (<=>)
  - `l2_distance` (<->)
  - `inner_product` (<#>)
- WHERE clause generation for vector filters
- ORDER BY SQL infrastructure in `order_by_generator.py:103-147`
- VectorOrderBy GraphQL input type in `graphql_order_by_generator.py:28-46`
- 5/6 integration tests passing

### ⏭️ Skipped Test
- `test_vector_order_by_distance` (line 129-133 in `test_vector_e2e.py`)
- Reason: "ORDER BY vector distance not yet implemented in integration test"
- Root cause: Test needs to use GraphQL input objects properly

### 📊 Current Architecture
```
Vector WHERE:
GraphQL Input (VectorFilter) → WHERE SQL (vectors.py) → PostgreSQL

Vector ORDER BY:
GraphQL Input (VectorOrderBy) → ORDER BY SQL (order_by_generator.py) → PostgreSQL
                    ↑
         INTEGRATION ISSUE HERE
```

## PHASES

---

### Phase 1: Fix Integration Test for ORDER BY Vector Distance

**Objective**: Un-skip and fix `test_vector_order_by_distance` integration test

**Root Cause Analysis**:
- Test comment says "requires GraphQL OrderByInput objects, not plain dicts"
- Current test at line 129-133 is skipped with pytest.skip()
- The infrastructure exists but test needs proper GraphQL input object usage

#### TDD Cycle 1.1: Write Proper Integration Test

**RED Phase**:
```python
# File: tests/integration/test_vector_e2e.py:129-150
@pytest.mark.asyncio
async def test_vector_order_by_distance(db_pool, vector_test_setup) -> None:
    """Test ordering results by vector distance using GraphQL input objects."""
    repo = FraiseQLRepository(db_pool)
    query_embedding = [0.1, 0.2, 0.3] + [0.0] * 381

    # Use proper GraphQL input object (not plain dict)
    from fraiseql.sql.graphql_order_by_generator import VectorOrderBy

    result = await repo.find(
        "test_documents",
        # Create VectorOrderBy input object
        orderBy={"embedding": VectorOrderBy(cosine_distance=query_embedding)},
        limit=3
    )

    results = extract_graphql_data(result, "test_documents")

    # Should return documents ordered by cosine distance
    assert len(results) == 3
    # First result should be Python Programming (identical embedding)
    assert results[0]["title"] == "Python Programming"
```

**Expected Failure**: Test should fail because VectorOrderBy input object is not being properly converted to OrderBy SQL in the repository layer.

**GREEN Phase**:
- Verify `_convert_order_by_input_to_sql` in `graphql_order_by_generator.py:92-195` handles VectorOrderBy correctly
- Check lines 136-161: VectorOrderBy processing logic exists
- May need to fix how repository passes order_by to SQL generator
- Minimal fix: Ensure VectorOrderBy instances are detected and converted

**Files to modify**:
- `tests/integration/test_vector_e2e.py:129-150` - Update test
- `src/fraiseql/db.py` or repository layer - Ensure proper conversion

**REFACTOR Phase**:
- Clean up conversion logic if needed
- Add type hints for clarity
- Ensure pattern follows existing WHERE clause conversion

**QA Phase**:
- [ ] Test passes with proper VectorOrderBy input
- [ ] Test works with all 3 distance operators (cosine, l2, inner_product)
- [ ] Integration with other ORDER BY fields works
- [ ] All existing tests still pass

---

### Phase 2: Add L1 Distance (Manhattan) Operator

**Objective**: Add support for pgvector's `<+>` L1/Manhattan distance operator

**Why L1 Distance**:
- Available in pgvector via `<+>` operator
- Useful for sparse vectors and categorical data
- Complements existing L2 distance (Euclidean)
- Natural progression: L2 → L1

#### TDD Cycle 2.1: Add L1 Distance to VectorFilter

**RED Phase**:
```python
# File: tests/unit/sql/where/operators/test_vector_operators.py
def test_l1_distance_filter():
    """Test L1/Manhattan distance operator generation."""
    from fraiseql.sql.where.operators.vectors import build_l1_distance_sql
    from psycopg.sql import SQL

    path_sql = SQL("data -> 'embedding'")
    vector = [0.1, 0.2, 0.3]

    result = build_l1_distance_sql(path_sql, vector)

    # Should generate: (data -> 'embedding')::vector <+> '[0.1,0.2,0.3]'::vector
    expected = "(data -> 'embedding')::vector <+> '[0.1,0.2,0.3]'::vector"
    assert str(result) == expected
```

**Expected Failure**: `ImportError: cannot import name 'build_l1_distance_sql'`

**GREEN Phase**:
```python
# File: src/fraiseql/sql/where/operators/vectors.py:49-57
def build_l1_distance_sql(path_sql: SQL, value: list[float]) -> Composed:
    """Build SQL for L1/Manhattan distance using PostgreSQL <+> operator.

    Generates: column <+> '[0.1,0.2,...]'::vector
    Returns distance: sum of absolute differences
    """
    vector_literal = "[" + ",".join(str(v) for v in value) + "]"
    return Composed([
        SQL("("), path_sql, SQL(")::vector <+> "),
        Literal(vector_literal), SQL("::vector")
    ])
```

**REFACTOR Phase**:
- Ensure consistent pattern with other vector operators
- Add comprehensive docstring with use cases
- Follow DRY principle if possible

**QA Phase**:
- [ ] Unit test passes
- [ ] SQL output is correct
- [ ] Type hints are accurate

#### TDD Cycle 2.2: Expose L1 in VectorFilter GraphQL Schema

**RED Phase**:
```python
# File: tests/integration/graphql/schema/test_vector_filter.py
def test_vector_filter_includes_l1_distance():
    """Test that VectorFilter includes l1_distance field."""
    from fraiseql.sql.graphql_where_generator import VectorFilter

    # VectorFilter should have l1_distance field
    assert hasattr(VectorFilter, 'l1_distance')

    # Should accept list[float]
    filter_input = VectorFilter(l1_distance=[0.1, 0.2, 0.3])
    assert filter_input.l1_distance == [0.1, 0.2, 0.3]
```

**Expected Failure**: `AttributeError: 'VectorFilter' has no attribute 'l1_distance'`

**GREEN Phase**:
```python
# File: src/fraiseql/sql/graphql_where_generator.py
# Update VectorFilter input type to include l1_distance
@fraise_input
class VectorFilter:
    """Filter input for vector/embedding fields using pgvector distance operators.

    Fields:
        cosine_distance: Cosine distance (0.0 = identical, 2.0 = opposite)
        l2_distance: L2/Euclidean distance (0.0 = identical, ∞ = different)
        l1_distance: L1/Manhattan distance (sum of absolute differences)
        inner_product: Negative inner product (more negative = more similar)
        isnull: Check if vector is NULL
    """
    cosine_distance: list[float] | None = None
    l2_distance: list[float] | None = None
    l1_distance: list[float] | None = None  # NEW
    inner_product: list[float] | None = None
    isnull: bool | None = None
```

**REFACTOR Phase**:
- Update operator registration in `__init__.py`
- Ensure GraphQL schema includes new field
- Update documentation strings

**QA Phase**:
- [ ] GraphQL schema test passes
- [ ] Field is properly typed
- [ ] Docstring is comprehensive

#### TDD Cycle 2.3: Integrate L1 into WHERE Clause Generation

**RED Phase**:
```python
# File: tests/integration/test_vector_e2e.py
@pytest.mark.asyncio
async def test_vector_l1_distance_filter(db_pool, vector_test_setup) -> None:
    """Test filtering documents by L1/Manhattan distance."""
    repo = FraiseQLRepository(db_pool)
    query_embedding = [0.1, 0.2, 0.3] + [0.0] * 381

    result = await repo.find(
        "test_documents",
        where={"embedding": {"l1_distance": query_embedding}},
        limit=5
    )

    results = extract_graphql_data(result, "test_documents")
    assert len(results) > 0
```

**Expected Failure**: May fail with "Unknown operator: l1_distance" or similar

**GREEN Phase**:
- Update operator mapping in `where/operators/__init__.py`
- Register `build_l1_distance_sql` function
- Ensure WHERE clause builder recognizes "l1_distance"

**Files to modify**:
- `src/fraiseql/sql/where/operators/__init__.py`
- WHERE clause generation logic

**REFACTOR Phase**:
- Ensure consistent operator registration pattern
- Clean up operator mapping dictionary
- Add inline documentation

**QA Phase**:
- [ ] Integration test passes
- [ ] L1 distance queries return correct results
- [ ] Composes with other filters
- [ ] Full test suite passes

#### TDD Cycle 2.4: Add L1 to VectorOrderBy

**RED Phase**:
```python
# File: tests/unit/sql/test_order_by_vector.py
def test_l1_distance_order_by():
    """Test ORDER BY L1 distance SQL generation."""
    from fraiseql.sql.order_by_generator import OrderBy

    order = OrderBy(
        field="embedding.l1_distance",
        value=[0.1, 0.2, 0.3]
    )

    sql = order.to_sql()

    # Should generate: (data -> 'embedding') <+> '[0.1,0.2,0.3]'::vector ASC
    assert "<+>" in str(sql)
    assert "[0.1,0.2,0.3]" in str(sql)
```

**Expected Failure**: L1 distance not recognized in `_build_vector_distance_sql`

**GREEN Phase**:
```python
# File: src/fraiseql/sql/order_by_generator.py:103-147
# Update _build_vector_distance_sql to handle l1_distance
def _build_vector_distance_sql(
    self, field_name: str, operator: str, value: list[float]
) -> sql.Composed:
    """Build SQL for vector distance ordering."""
    # Map operator names to PostgreSQL operators
    if operator == "cosine_distance":
        pg_operator_sql = sql.SQL("<=>")
    elif operator == "l2_distance":
        pg_operator_sql = sql.SQL("<->")
    elif operator == "l1_distance":  # NEW
        pg_operator_sql = sql.SQL("<+>")
    elif operator == "inner_product":
        pg_operator_sql = sql.SQL("<#>")
    else:
        raise ValueError(f"Unknown vector distance operator: {operator}")

    # ... rest of implementation
```

**REFACTOR Phase**:
- Add comprehensive docstring updates
- Ensure operator mapping is maintainable
- Consider extracting operator map to constant

**QA Phase**:
- [ ] Unit test passes
- [ ] Integration test with ORDER BY works
- [ ] Composes with other ORDER BY fields

#### TDD Cycle 2.5: Update VectorOrderBy GraphQL Input

**RED Phase**:
```python
# File: tests/integration/graphql/schema/test_vector_order_by.py
def test_vector_order_by_includes_l1():
    """Test that VectorOrderBy includes l1_distance field."""
    from fraiseql.sql.graphql_order_by_generator import VectorOrderBy

    order_input = VectorOrderBy(l1_distance=[0.1, 0.2, 0.3])
    assert order_input.l1_distance == [0.1, 0.2, 0.3]
```

**Expected Failure**: `AttributeError: 'VectorOrderBy' has no attribute 'l1_distance'`

**GREEN Phase**:
```python
# File: src/fraiseql/sql/graphql_order_by_generator.py:28-46
@fraise_input
class VectorOrderBy:
    """Order by input for vector/embedding fields using pgvector distance operators.

    Fields:
        cosine_distance: Order by cosine distance
        l2_distance: Order by L2/Euclidean distance
        l1_distance: Order by L1/Manhattan distance
        inner_product: Order by negative inner product
    """
    cosine_distance: list[float] | None = None
    l2_distance: list[float] | None = None
    l1_distance: list[float] | None = None  # NEW
    inner_product: list[float] | None = None
```

**REFACTOR Phase**:
- Update conversion logic in `_convert_order_by_input_to_sql`
- Add l1_distance handling in lines 146-161

**QA Phase**:
- [ ] GraphQL schema test passes
- [ ] End-to-end ORDER BY test passes
- [ ] Documentation updated

#### TDD Cycle 2.6: End-to-End Integration Test

**RED Phase**:
```python
# File: tests/integration/test_vector_e2e.py
@pytest.mark.asyncio
async def test_vector_l1_end_to_end(db_pool, vector_test_setup) -> None:
    """Test L1 distance for both WHERE and ORDER BY."""
    repo = FraiseQLRepository(db_pool)
    query_embedding = [0.1, 0.2, 0.3] + [0.0] * 381

    from fraiseql.sql.graphql_order_by_generator import VectorOrderBy

    result = await repo.find(
        "test_documents",
        where={"embedding": {"l1_distance": query_embedding}},
        orderBy={"embedding": VectorOrderBy(l1_distance=query_embedding)},
        limit=3
    )

    results = extract_graphql_data(result, "test_documents")

    assert len(results) == 3
    # Results should be ordered by L1 distance
    assert results[0]["title"] == "Python Programming"
```

**GREEN Phase**: Should pass if all previous cycles completed successfully

**REFACTOR Phase**:
- Clean up test structure
- Add comments explaining L1 use case
- Ensure test is maintainable

**QA Phase**:
- [ ] Full integration test passes
- [ ] WHERE + ORDER BY composition works
- [ ] All existing tests still pass
- [ ] Documentation updated

---

### Phase 3: Add Binary Vector Operators (Hamming & Jaccard)

**Objective**: Add support for pgvector's binary vector distance operators

**Why Binary Operators**:
- Hamming distance (`<~>`) - for bit vectors, counts differing bits
- Jaccard distance (`<%>`) - for set similarity with bit vectors
- Useful for categorical/binary features, fingerprints, hash-based similarity
- Enables new use cases beyond continuous embeddings

**Note**: These operators work on `bit` type vectors, not float vectors

#### TDD Cycle 3.1: Add Hamming Distance Operator

**RED Phase**:
```python
# File: tests/unit/sql/where/operators/test_vector_operators.py
def test_hamming_distance_filter():
    """Test Hamming distance operator for bit vectors."""
    from fraiseql.sql.where.operators.vectors import build_hamming_distance_sql
    from psycopg.sql import SQL

    path_sql = SQL("data -> 'fingerprint'")
    # Hamming works on bit vectors, represented as strings
    bit_vector = "101010"  # 6-bit vector

    result = build_hamming_distance_sql(path_sql, bit_vector)

    # Should generate: (data -> 'fingerprint')::bit <~> '101010'::bit
    expected = "(data -> 'fingerprint')::bit <~> '101010'::bit"
    assert str(result) == expected
```

**Expected Failure**: `ImportError: cannot import name 'build_hamming_distance_sql'`

**GREEN Phase**:
```python
# File: src/fraiseql/sql/where/operators/vectors.py:58-68
def build_hamming_distance_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Hamming distance using PostgreSQL <~> operator.

    Generates: column <~> '101010'::bit
    Returns distance: number of differing bits

    Note: Hamming distance works on bit type vectors, not float vectors.
    Use for categorical features, fingerprints, or binary similarity.
    """
    return Composed([
        SQL("("), path_sql, SQL(")::bit <~> "),
        Literal(value), SQL("::bit")
    ])
```

**REFACTOR Phase**:
- Add type handling for bit vectors vs float vectors
- Consider field name pattern detection for bit vectors
- Update docstrings with use cases

**QA Phase**:
- [ ] Unit test passes
- [ ] SQL output is correct
- [ ] Type hints handle str input for bit vectors

#### TDD Cycle 3.2: Add Jaccard Distance Operator

**RED Phase**:
```python
# File: tests/unit/sql/where/operators/test_vector_operators.py
def test_jaccard_distance_filter():
    """Test Jaccard distance operator for bit vectors."""
    from fraiseql.sql.where.operators.vectors import build_jaccard_distance_sql
    from psycopg.sql import SQL

    path_sql = SQL("data -> 'features'")
    bit_vector = "111000"

    result = build_jaccard_distance_sql(path_sql, bit_vector)

    # Should generate: (data -> 'features')::bit <%> '111000'::bit
    expected = "(data -> 'features')::bit <%> '111000'::bit"
    assert str(result) == expected
```

**Expected Failure**: `ImportError: cannot import name 'build_jaccard_distance_sql'`

**GREEN Phase**:
```python
# File: src/fraiseql/sql/where/operators/vectors.py:69-79
def build_jaccard_distance_sql(path_sql: SQL, value: str) -> Composed:
    """Build SQL for Jaccard distance using PostgreSQL <%> operator.

    Generates: column <%> '111000'::bit
    Returns distance: 1 - (intersection / union) for bit sets

    Note: Jaccard distance works on bit type vectors for set similarity.
    Useful for recommendation systems, tag similarity, feature matching.
    """
    return Composed([
        SQL("("), path_sql, SQL(")::bit <%> "),
        Literal(value), SQL("::bit")
    ])
```

**REFACTOR Phase**:
- Ensure consistent pattern with Hamming
- Add comprehensive examples
- Document bit vector representation

**QA Phase**:
- [ ] Unit test passes
- [ ] SQL generation is correct
- [ ] Documentation is clear

#### TDD Cycle 3.3: Update VectorFilter Schema for Binary Operators

**RED Phase**:
```python
# File: tests/integration/graphql/schema/test_vector_filter.py
def test_vector_filter_binary_operators():
    """Test that VectorFilter supports binary vector operators."""
    from fraiseql.sql.graphql_where_generator import VectorFilter

    # Should support hamming_distance with string input
    filter_input = VectorFilter(hamming_distance="101010")
    assert filter_input.hamming_distance == "101010"

    # Should support jaccard_distance with string input
    filter_input2 = VectorFilter(jaccard_distance="111000")
    assert filter_input2.jaccard_distance == "111000"
```

**Expected Failure**: `AttributeError: 'VectorFilter' has no attributes for binary operators`

**GREEN Phase**:
```python
# File: src/fraiseql/sql/graphql_where_generator.py
@fraise_input
class VectorFilter:
    """Filter input for vector/embedding fields.

    Supports both continuous (float) and binary (bit) vector operations.

    Float Vector Operators:
        cosine_distance: Cosine distance (0.0 = identical, 2.0 = opposite)
        l2_distance: L2/Euclidean distance
        l1_distance: L1/Manhattan distance
        inner_product: Negative inner product

    Binary Vector Operators:
        hamming_distance: Hamming distance for bit vectors (count differing bits)
        jaccard_distance: Jaccard distance for set similarity (1 - intersection/union)

    Other:
        isnull: Check if vector is NULL
    """
    # Float vector operators
    cosine_distance: list[float] | None = None
    l2_distance: list[float] | None = None
    l1_distance: list[float] | None = None
    inner_product: list[float] | None = None

    # Binary vector operators
    hamming_distance: str | None = None  # NEW - bit string like "101010"
    jaccard_distance: str | None = None  # NEW - bit string like "111000"

    # Common
    isnull: bool | None = None
```

**REFACTOR Phase**:
- Update operator registration
- Add type validation for bit strings
- Document bit vector format

**QA Phase**:
- [ ] Schema test passes
- [ ] GraphQL accepts str input for binary operators
- [ ] Type hints are accurate

#### TDD Cycle 3.4: Integration Tests for Binary Operators

**RED Phase**:
```python
# File: tests/integration/test_vector_binary.py
"""Integration tests for binary vector operators (Hamming, Jaccard)."""

@pytest.fixture
async def binary_vector_test_setup(db_pool) -> None:
    """Set up test database with bit vector columns."""
    async with db_pool.connection() as conn:
        await conn.execute("CREATE EXTENSION IF NOT EXISTS vector;")

        # Create table with bit vector column
        await conn.execute("""
            DROP TABLE IF EXISTS test_fingerprints CASCADE;
            CREATE TABLE test_fingerprints (
                id UUID PRIMARY KEY,
                name TEXT,
                fingerprint bit(64),  -- 64-bit vector
                tenant_id UUID
            )
        """)

        # Insert test data with bit vectors
        test_data = [
            {
                "id": "550e8400-e29b-41d4-a716-446655440001",
                "name": "Item A",
                "fingerprint": "1111000011110000111100001111000011110000111100001111000011110000",
                "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
            },
            {
                "id": "550e8400-e29b-41d4-a716-446655440002",
                "name": "Item B",
                "fingerprint": "1111111100000000111111110000000011111111000000001111111100000000",
                "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
            },
        ]

        for item in test_data:
            await conn.execute(
                """
                INSERT INTO test_fingerprints (id, name, fingerprint, tenant_id)
                VALUES (%s, %s, %s::bit(64), %s)
                """,
                (item["id"], item["name"], item["fingerprint"], item["tenant_id"]),
            )

        await conn.commit()


@pytest.mark.asyncio
async def test_hamming_distance_filter(db_pool, binary_vector_test_setup) -> None:
    """Test filtering by Hamming distance."""
    repo = FraiseQLRepository(db_pool)
    query_fingerprint = "1111000011110000111100001111000011110000111100001111000011110000"

    result = await repo.find(
        "test_fingerprints",
        where={"fingerprint": {"hamming_distance": query_fingerprint}},
        limit=5
    )

    results = extract_graphql_data(result, "test_fingerprints")
    assert len(results) > 0
    # Item A should match exactly (Hamming distance = 0)
    assert results[0]["name"] == "Item A"


@pytest.mark.asyncio
async def test_jaccard_distance_filter(db_pool, binary_vector_test_setup) -> None:
    """Test filtering by Jaccard distance."""
    repo = FraiseQLRepository(db_pool)
    query_fingerprint = "1111000011110000111100001111000011110000111100001111000011110000"

    result = await repo.find(
        "test_fingerprints",
        where={"fingerprint": {"jaccard_distance": query_fingerprint}},
        limit=5
    )

    results = extract_graphql_data(result, "test_fingerprints")
    assert len(results) > 0
```

**Expected Failure**: Operators not registered in WHERE clause generation

**GREEN Phase**:
- Register `hamming_distance` and `jaccard_distance` in operator mapping
- Update WHERE clause builder to handle string (bit) values
- Ensure proper SQL type casting

**Files to modify**:
- `src/fraiseql/sql/where/operators/__init__.py`
- WHERE clause generation logic

**REFACTOR Phase**:
- Clean up operator registration pattern
- Add type discrimination for float vs bit vectors
- Improve error messages for type mismatches

**QA Phase**:
- [ ] Integration tests pass
- [ ] Binary operators work with WHERE clauses
- [ ] Type handling is correct (str for bits, list[float] for vectors)
- [ ] Full test suite passes

#### TDD Cycle 3.5: Add Binary Operators to ORDER BY

**Similar pattern to L1 distance cycles 2.4-2.5**

**RED Phase**: Write failing tests for ORDER BY with Hamming/Jaccard

**GREEN Phase**:
- Update `_build_vector_distance_sql` to handle hamming_distance and jaccard_distance
- Add operators to VectorOrderBy GraphQL input
- Update conversion logic

**REFACTOR Phase**: Clean up and document

**QA Phase**: Verify end-to-end functionality

---

### Phase 4: Documentation and Examples

**Objective**: Update all documentation to reflect new operators

#### TDD Cycle 4.1: Update Feature Documentation

**Files to update**:
- `docs/features/pgvector.md` - Add L1, Hamming, Jaccard sections
- `docs/examples/semantic-search.md` - Add examples with new operators
- `README.md` - Mention expanded operator support

**Updates needed**:
1. **VectorFilter Schema** section - add new operators
2. **Distance Operators** section - add L1, Hamming, Jaccard subsections
3. **Use Cases** section - add binary vector use cases:
   - Fingerprint matching (Hamming)
   - Tag/category similarity (Jaccard)
   - Feature matching (both)
4. **Code Examples** - show binary vector usage

**Example Addition**:
```markdown
#### L1 Distance (`l1_distance`)
- **Operator**: `<+>` (L1/Manhattan distance)
- **Range**: 0.0 (identical) to ∞ (very different)
- **Use case**: Sparse vectors, grid-based distances

#### Hamming Distance (`hamming_distance`)
- **Operator**: `<~>` (Hamming distance)
- **Type**: Binary vectors (bit type)
- **Range**: 0 (identical) to N (all bits differ, where N = vector length)
- **Use case**: Fingerprint matching, binary features, hashing

#### Jaccard Distance (`jaccard_distance`)
- **Operator**: `<%>` (Jaccard distance)
- **Type**: Binary vectors (bit type)
- **Range**: 0.0 (identical sets) to 1.0 (no overlap)
- **Use case**: Set similarity, tag matching, recommendation systems
```

#### TDD Cycle 4.2: Add Binary Vector Examples

**New documentation file**: `docs/examples/binary-vectors.md`

**Content**:
- Setup with bit vector columns
- Hamming distance for fingerprint matching
- Jaccard distance for tag similarity
- Combined filters with WHERE + ORDER BY
- Performance considerations for bit vectors
- Index setup for binary vectors

**Example**:
```python
# Fingerprint matching with Hamming distance
@fraise_type
class ImageFingerprint:
    id: UUID
    name: str
    fingerprint: str  # bit(256) in PostgreSQL
    category: str

# Find similar fingerprints
similar = await repo.find(
    "image_fingerprints",
    where={
        "fingerprint": {"hamming_distance": query_fingerprint},
        "category": {"eq": "portraits"}
    },
    orderBy={"fingerprint": VectorOrderBy(hamming_distance=query_fingerprint)},
    limit=10
)
```

#### TDD Cycle 4.3: Update README

**File**: `README.md`

**Changes**:
- Update feature list to mention "6 vector distance operators"
- Add brief mention of binary vector support
- Link to expanded pgvector documentation

**QA Phase**:
- [ ] All documentation is accurate
- [ ] Examples are tested and working
- [ ] Links are correct
- [ ] Documentation follows project style

---

## Success Criteria

### Phase 1 Complete
- [ ] `test_vector_order_by_distance` passes (not skipped)
- [ ] ORDER BY vector distance works with GraphQL input objects
- [ ] Integration with WHERE + ORDER BY works
- [ ] All 6/6 integration tests passing

### Phase 2 Complete
- [ ] L1 distance operator implemented for WHERE
- [ ] L1 distance operator implemented for ORDER BY
- [ ] VectorFilter includes `l1_distance` field
- [ ] VectorOrderBy includes `l1_distance` field
- [ ] Integration tests pass for L1 distance
- [ ] Documentation updated with L1 examples

### Phase 3 Complete
- [ ] Hamming distance operator implemented
- [ ] Jaccard distance operator implemented
- [ ] Binary vector integration tests pass
- [ ] Type handling works (str for bits, list[float] for vectors)
- [ ] Documentation includes binary vector guide

### Phase 4 Complete
- [ ] Feature documentation updated
- [ ] Binary vector examples added
- [ ] README updated
- [ ] All examples tested and working

### Overall Success
- [ ] All tests passing (unit + integration)
- [ ] Code quality standards met (ruff, mypy)
- [ ] 6 vector distance operators supported:
  - cosine_distance (<=>)
  - l2_distance (<->)
  - l1_distance (<+>)
  - inner_product (<#>)
  - hamming_distance (<~>)
  - jaccard_distance (<%>)
- [ ] Both WHERE and ORDER BY support all operators
- [ ] GraphQL schema properly exposes all operators
- [ ] Documentation is comprehensive and accurate
- [ ] FraiseQL philosophy maintained (thin layer, PostgreSQL-first)

---

## Implementation Notes

### Type Handling Strategy
- **Float vectors**: `list[float]` in Python, `vector(N)` in PostgreSQL
- **Binary vectors**: `str` in Python (e.g., "101010"), `bit(N)` in PostgreSQL
- Field detection remains same (name patterns), but type determines available operators

### Operator Registration Pattern
- Each operator has dedicated `build_*_sql` function in `vectors.py`
- Registration in `where/operators/__init__.py` maps GraphQL field to SQL builder
- ORDER BY uses same pattern in `order_by_generator.py`

### GraphQL Schema Evolution
- VectorFilter grows to 6 distance operator fields + isnull
- VectorOrderBy grows to 6 distance operator fields
- Backward compatible (all fields are Optional)

### Testing Strategy
1. **Unit tests**: SQL generation for each operator
2. **Schema tests**: GraphQL input types include new fields
3. **Integration tests**: End-to-end with PostgreSQL + pgvector
4. **E2E tests**: Combined WHERE + ORDER BY scenarios

### Performance Considerations
- Binary operators typically faster than float vector operators
- HNSW indexes support cosine, L2, inner product
- IVFFlat indexes support all float operators
- Bit indexes use GIN or GiST for binary operators
- Document index requirements for each operator type

---

## Risk Mitigation

### Risk: Type Confusion (float vs bit vectors)
**Mitigation**:
- Clear type hints (list[float] vs str)
- Explicit error messages for type mismatches
- Comprehensive documentation explaining when to use each

### Risk: PostgreSQL Version Compatibility
**Mitigation**:
- Document minimum pgvector version for each operator
- Graceful degradation if operator not supported
- Clear error messages pointing to pgvector documentation

### Risk: Breaking Changes to GraphQL Schema
**Mitigation**:
- All new fields are optional (backward compatible)
- Existing queries continue to work
- Version documentation clearly

### Risk: Binary Vector Representation
**Mitigation**:
- Document bit string format clearly ("101010")
- Provide validation examples
- Show conversion from common formats (hex, bytes)

---

## Timeline Estimate

- **Phase 1**: 2-3 hours (simple test fix)
- **Phase 2**: 4-6 hours (L1 operator, full integration)
- **Phase 3**: 6-8 hours (binary operators, new types, more complex)
- **Phase 4**: 2-3 hours (documentation)

**Total**: 14-20 hours

---

## Dependencies

- PostgreSQL 11+ with pgvector extension
- pgvector version supporting all operators (check version for L1, binary ops)
- Existing FraiseQL vector infrastructure from Phase 1
- Test database with vector extension enabled

---

## Future Enhancements (Out of Scope)

- Sparse vector support (`sparsevec` type)
- Half-precision vectors (`halfvec` type)
- Vector aggregation functions
- Custom distance functions
- Vector quantization
- Multi-vector fields per type

---

*Generated: 2025-11-13*
*Status: Ready for Implementation*
*Approach: Phased TDD Development*
