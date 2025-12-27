# pgvector Phase 3 Implementation Plan - Complete Feature Set

**Status**: Planning
**Complexity**: Complex | **Phased TDD Approach**
**Estimated Time**: 52-70 hours (Realistic: 61 hours)

## Executive Summary

This plan completes FraiseQL's pgvector implementation by adding the remaining advanced features that will establish FraiseQL as the most comprehensive GraphQL framework for vector operations. After completion, FraiseQL will have **complete pgvector feature parity** and become the de facto standard for Python AI/ML GraphQL applications.

### What's Already Complete (v1.5.0)

✅ **Phase 1 & 2 Complete:**
- 6 vector distance operators (cosine, L2, L1, inner product, Hamming, Jaccard)
- VectorFilter GraphQL input type (WHERE clauses)
- VectorOrderBy GraphQL input type (ORDER BY clauses)
- Binary vector support (bit type)
- Full integration tests (13/13 passing)
- Production-ready with comprehensive test coverage

### Phase 3 Objectives

This phase adds 5 advanced features:

1. **Half-precision vectors** (`halfvec`) - 50% memory reduction
2. **Sparse vectors** (`sparsevec`) - High-dimensional sparse data
3. **Vector aggregations** - Centroid calculation, batch operations
4. **Custom distance functions** - User-defined similarity metrics
5. **Vector quantization** - Memory/performance optimization

### Success Criteria

- [ ] All 5 features implemented with full TDD coverage
- [ ] GraphQL schema auto-generation for all new types
- [ ] Integration tests for all features (>95% coverage)
- [ ] Performance benchmarks published
- [ ] Documentation complete with examples
- [ ] Production-ready code quality

## Current State Analysis

### Architecture Foundation (v1.5.0)

```
Python Types → GraphQL Schema → SQL Generation → PostgreSQL
     ↓              ↓                 ↓              ↓
Field Detection  Auto-gen Input   psycopg3 SQL   pgvector ops
     ↓              ↓                 ↓              ↓
Vector patterns  VectorFilter    vectors.py     Native pgvector
```

**Strengths:**
- ✅ Proven TDD methodology
- ✅ Clean separation of concerns
- ✅ Type-safe schema generation
- ✅ Production-ready test infrastructure

**Extension Points for Phase 3:**
- `src/fraiseql/sql/where/operators/vectors.py` - Add new operators
- `src/fraiseql/sql/graphql_where_generator.py` - Schema generation
- `src/fraiseql/sql/order_by_generator.py` - ORDER BY support
- `src/fraiseql/core/graphql_type.py` - Field detection

## PHASES

---

## Phase 3.1: Half-Precision Vectors (`halfvec`)

**Objective**: Add support for 16-bit float vectors (50% memory reduction)
**Estimated Time**: 6-8 hours
**Complexity**: Medium
**Dependencies**: pgvector >= 0.5.0

### Background

PostgreSQL `halfvec` type:
- Stores vectors as 16-bit floats instead of 32-bit
- 50% memory reduction
- Slight precision loss (acceptable for most use cases)
- Same operators as regular vectors
- Use case: Large-scale deployments (100M+ vectors)

### TDD Cycle 3.1.1: Field Detection for halfvec

**Objective**: Auto-detect half-precision vector fields

#### RED Phase

**Test:** `tests/unit/core/test_field_detection_halfvec.py`

```python
import pytest
from fraiseql.core.graphql_type import _should_use_vector_operators

def test_halfvec_field_detection_by_name():
    """Test that halfvec fields are detected by naming convention."""
    # These should be detected as half-precision vectors
    assert _should_use_vector_operators("embedding_half") is True
    assert _should_use_vector_operators("vector_fp16") is True
    assert _should_use_vector_operators("halfvec_embedding") is True
    assert _should_use_vector_operators("compact_embedding") is True

def test_halfvec_vs_regular_vector_distinction():
    """Test that we can distinguish halfvec from regular vectors."""
    from fraiseql.core.graphql_type import _detect_vector_type

    # Regular vector patterns
    assert _detect_vector_type("embedding") == "vector"
    assert _detect_vector_type("vector") == "vector"

    # Half-precision patterns
    assert _detect_vector_type("embedding_half") == "halfvec"
    assert _detect_vector_type("vector_fp16") == "halfvec"
    assert _detect_vector_type("compact_embedding") == "halfvec"

def test_halfvec_type_hint_detection():
    """Test detection via type hints (when available)."""
    from typing import Annotated
    import fraiseql as fraiseql_type

    @fraiseql_type
    class DocumentHalfVec:
        id: int
        embedding: Annotated[list[float], "halfvec"]  # Explicit annotation

    # Should detect halfvec from annotation
    fields = DocumentHalfVec._fraiseql_fields
    assert fields["embedding"].vector_type == "halfvec"
```

**Expected Failure**: Functions `_detect_vector_type()` and halfvec field detection don't exist yet.

**Run Test:**
```bash
uv run pytest tests/unit/core/test_field_detection_halfvec.py -xvs
# Expected: FAILED - Functions not implemented
```

#### GREEN Phase

**Implementation:** `src/fraiseql/core/graphql_type.py`

```python
def _detect_vector_type(field_name: str) -> str | None:
    """Detect the type of vector field (vector, halfvec, sparsevec).

    Returns:
        - "vector": Regular 32-bit float vector
        - "halfvec": 16-bit float vector (half-precision)
        - "sparsevec": Sparse vector
        - None: Not a vector field
    """
    field_lower = field_name.lower()

    # Half-precision vector patterns
    halfvec_patterns = {
        "half", "fp16", "float16", "compact", "halfvec",
        "half_precision", "16bit"
    }
    if any(pattern in field_lower for pattern in halfvec_patterns):
        return "halfvec"

    # Sparse vector patterns (Phase 3.2)
    sparse_patterns = {"sparse", "sparsevec"}
    if any(pattern in field_lower for pattern in sparse_patterns):
        return "sparsevec"

    # Regular vector patterns
    vector_patterns = {
        "embedding", "vector", "vec", "feature",
        "representation", "latent", "encoded"
    }
    if any(pattern in field_lower for pattern in vector_patterns):
        return "vector"

    return None

def _should_use_vector_operators(field_name: str) -> bool:
    """Check if field should use vector operators (any vector type)."""
    return _detect_vector_type(field_name) is not None
```

**Run Test:**
```bash
uv run pytest tests/unit/core/test_field_detection_halfvec.py -xvs
# Expected: PASSED
```

#### REFACTOR Phase

**Improvements:**
1. Extract patterns to module-level constants for maintainability
2. Add docstrings with examples
3. Ensure backward compatibility with existing vector detection

**Code Quality:**
```python
# Module-level constants for clarity
HALFVEC_PATTERNS = frozenset({
    "half", "fp16", "float16", "compact", "halfvec",
    "half_precision", "16bit"
})

VECTOR_PATTERNS = frozenset({
    "embedding", "vector", "vec", "feature",
    "representation", "latent", "encoded"
})

SPARSE_PATTERNS = frozenset({
    "sparse", "sparsevec"
})

def _detect_vector_type(field_name: str) -> str | None:
    """Detect vector field type by naming convention.

    Examples:
        >>> _detect_vector_type("embedding")
        'vector'
        >>> _detect_vector_type("embedding_half")
        'halfvec'
        >>> _detect_vector_type("sparse_features")
        'sparsevec'
        >>> _detect_vector_type("title")
        None
    """
    field_lower = field_name.lower()

    if any(pattern in field_lower for pattern in HALFVEC_PATTERNS):
        return "halfvec"

    if any(pattern in field_lower for pattern in SPARSE_PATTERNS):
        return "sparsevec"

    if any(pattern in field_lower for pattern in VECTOR_PATTERNS):
        return "vector"

    return None
```

**Run Tests:**
```bash
uv run pytest tests/unit/core/test_field_detection_halfvec.py -v
# All tests should still pass
```

#### QA Phase

**Verification:**
```bash
# Run all field detection tests
uv run pytest tests/unit/core/test_field_detection*.py -v

# Run type checking
uv run mypy src/fraiseql/core/graphql_type.py

# Run linting
uv run ruff check src/fraiseql/core/graphql_type.py
```

**Success Criteria:**
- [ ] All field detection tests pass
- [ ] No type errors
- [ ] No linting issues
- [ ] Backward compatibility maintained (existing tests still pass)

---

### TDD Cycle 3.1.2: halfvec SQL Generation

**Objective**: Generate correct SQL for halfvec operations

#### RED Phase

**Test:** `tests/unit/sql/test_halfvec_operators.py`

```python
import pytest
from fraiseql.sql.where.operators.vectors import (
    build_cosine_distance_sql,
    build_l2_distance_sql,
)
from psycopg.sql import SQL, Identifier

def test_halfvec_cosine_distance_sql():
    """Test SQL generation for halfvec cosine distance."""
    path_sql = SQL("t.").join([Identifier("embedding_half")])
    query_embedding = [0.1, 0.2, 0.3, 0.4]

    sql = build_cosine_distance_sql(path_sql, query_embedding, vector_type="halfvec")
    sql_string = sql.as_string(None)

    # Should cast to halfvec instead of vector
    assert "::halfvec" in sql_string
    assert "<=> '[0.1,0.2,0.3,0.4]'::halfvec" in sql_string

def test_halfvec_l2_distance_sql():
    """Test SQL generation for halfvec L2 distance."""
    path_sql = SQL("t.").join([Identifier("compact_vector")])
    query_vector = [0.5, 0.5, 0.5, 0.5]

    sql = build_l2_distance_sql(path_sql, query_vector, vector_type="halfvec")
    sql_string = sql.as_string(None)

    assert "::halfvec" in sql_string
    assert "<-> '[0.5,0.5,0.5,0.5]'::halfvec" in sql_string

def test_regular_vector_backward_compatibility():
    """Ensure regular vectors still work (backward compatibility)."""
    path_sql = SQL("t.").join([Identifier("embedding")])
    query_embedding = [0.1, 0.2]

    # Default should still be 'vector'
    sql = build_cosine_distance_sql(path_sql, query_embedding)
    sql_string = sql.as_string(None)

    assert "::vector" in sql_string
    assert "::halfvec" not in sql_string
```

**Expected Failure**: Functions don't accept `vector_type` parameter yet.

**Run Test:**
```bash
uv run pytest tests/unit/sql/test_halfvec_operators.py -xvs
# Expected: FAILED - Missing parameter
```

#### GREEN Phase

**Implementation:** `src/fraiseql/sql/where/operators/vectors.py`

```python
def build_cosine_distance_sql(
    path_sql: SQL,
    value: list[float],
    vector_type: str = "vector"
) -> Composed:
    """Build SQL for cosine distance with configurable vector type.

    Args:
        path_sql: SQL path to the vector column
        value: Query vector values
        vector_type: One of "vector", "halfvec", "sparsevec"

    Generates:
        - Regular: (column)::vector <=> '[0.1,0.2,...]'::vector
        - Half-precision: (column)::halfvec <=> '[0.1,0.2,...]'::halfvec

    Returns distance: 0.0 (identical) to 2.0 (opposite)
    """
    vector_literal = "[" + ",".join(str(v) for v in value) + "]"
    cast_type = SQL(f"::{vector_type}")

    return Composed([
        SQL("("),
        path_sql,
        SQL(")"),
        cast_type,
        SQL(" <=> "),
        Literal(vector_literal),
        cast_type
    ])

def build_l2_distance_sql(
    path_sql: SQL,
    value: list[float],
    vector_type: str = "vector"
) -> Composed:
    """Build SQL for L2 distance with configurable vector type."""
    vector_literal = "[" + ",".join(str(v) for v in value) + "]"
    cast_type = SQL(f"::{vector_type}")

    return Composed([
        SQL("("),
        path_sql,
        SQL(")"),
        cast_type,
        SQL(" <-> "),
        Literal(vector_literal),
        cast_type
    ])

# Update all other vector operators (inner_product, l1, hamming, jaccard)
# to accept vector_type parameter with same pattern
```

**Run Test:**
```bash
uv run pytest tests/unit/sql/test_halfvec_operators.py -xvs
# Expected: PASSED
```

#### REFACTOR Phase

**Improvements:**
1. DRY: Extract common pattern for all operators
2. Add type validation for vector_type parameter
3. Update all 6 distance operators consistently

**Refactored Code:**
```python
from typing import Literal

VectorType = Literal["vector", "halfvec", "sparsevec"]

def _build_vector_distance_sql(
    path_sql: SQL,
    value: list[float],
    operator: str,
    vector_type: VectorType = "vector"
) -> Composed:
    """Generic vector distance SQL builder.

    Args:
        path_sql: SQL path to the vector column
        value: Query vector values
        operator: Distance operator (<=> | <-> | <#> | <+>)
        vector_type: Vector type for casting
    """
    vector_literal = "[" + ",".join(str(v) for v in value) + "]"
    cast_type = SQL(f"::{vector_type}")

    return Composed([
        SQL("("),
        path_sql,
        SQL(")"),
        cast_type,
        SQL(f" {operator} "),
        Literal(vector_literal),
        cast_type
    ])

def build_cosine_distance_sql(
    path_sql: SQL,
    value: list[float],
    vector_type: VectorType = "vector"
) -> Composed:
    """Build SQL for cosine distance."""
    return _build_vector_distance_sql(path_sql, value, "<=>", vector_type)

def build_l2_distance_sql(
    path_sql: SQL,
    value: list[float],
    vector_type: VectorType = "vector"
) -> Composed:
    """Build SQL for L2/Euclidean distance."""
    return _build_vector_distance_sql(path_sql, value, "<->", vector_type)

def build_inner_product_sql(
    path_sql: SQL,
    value: list[float],
    vector_type: VectorType = "vector"
) -> Composed:
    """Build SQL for inner product."""
    return _build_vector_distance_sql(path_sql, value, "<#>", vector_type)

def build_l1_distance_sql(
    path_sql: SQL,
    value: list[float],
    vector_type: VectorType = "vector"
) -> Composed:
    """Build SQL for L1/Manhattan distance."""
    return _build_vector_distance_sql(path_sql, value, "<+>", vector_type)
```

**Run Tests:**
```bash
uv run pytest tests/unit/sql/test_halfvec_operators.py -v
uv run pytest tests/unit/sql/test_order_by_vector.py -v  # Ensure no regression
```

#### QA Phase

**Verification:**
```bash
# Run all vector operator tests
uv run pytest tests/unit/sql/test_*vector*.py -v

# Type checking
uv run mypy src/fraiseql/sql/where/operators/vectors.py

# Integration test (will add in next cycle)
uv run pytest tests/integration/test_vector_e2e.py -v
```

---

### TDD Cycle 3.1.3: halfvec Integration Tests

**Objective**: End-to-end testing with real PostgreSQL

#### RED Phase

**Test:** `tests/integration/test_halfvec_e2e.py`

```python
import pytest
import pytest_asyncio
import fraiseql as fraiseql_type
from fraiseql.db import FraiseQLRepository

@fraiseql_type
class DocumentHalfVec:
    """Document with half-precision embedding."""
    id: int
    title: str
    embedding_half: list[float]  # Detected as halfvec

@pytest_asyncio.fixture
async def halfvec_test_setup(db_pool):
    """Set up test table with halfvec column."""
    async with db_pool.connection() as conn:
        # Create table with halfvec column
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS test_documents_halfvec (
                id SERIAL PRIMARY KEY,
                title TEXT,
                embedding_half halfvec(384)
            )
        """)

        # Insert test data
        await conn.execute("""
            INSERT INTO test_documents_halfvec (title, embedding_half)
            VALUES
                ('Python Programming', array_fill(0.1, ARRAY[384])::halfvec),
                ('Java Tutorial', array_fill(0.5, ARRAY[384])::halfvec),
                ('C++ Guide', array_fill(0.9, ARRAY[384])::halfvec)
        """)

        # Create HNSW index for halfvec
        await conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_halfvec_embedding
            ON test_documents_halfvec
            USING hnsw (embedding_half halfvec_cosine_ops)
        """)

        yield

        # Cleanup
        await conn.execute("DROP TABLE IF EXISTS test_documents_halfvec CASCADE")

@pytest.mark.asyncio
async def test_halfvec_cosine_distance_filter(db_pool, halfvec_test_setup):
    """Test filtering with halfvec cosine distance."""
    repo = FraiseQLRepository(db_pool)
    query_embedding = [0.1] * 384

    result = await repo.find(
        "test_documents_halfvec",
        where={"embedding_half": {"cosine_distance": query_embedding}},
        limit=3
    )

    results = result.to_json()["data"]["test_documents_halfvec"]

    # Should find documents ordered by similarity
    assert len(results) == 3
    assert results[0]["title"] == "Python Programming"  # Closest

@pytest.mark.asyncio
async def test_halfvec_order_by_distance(db_pool, halfvec_test_setup):
    """Test ordering by halfvec distance."""
    from fraiseql.sql.graphql_order_by_generator import VectorOrderBy

    repo = FraiseQLRepository(db_pool)
    query_embedding = [0.5] * 384

    result = await repo.find(
        "test_documents_halfvec",
        orderBy={"embedding_half": VectorOrderBy(cosine_distance=query_embedding)},
        limit=3
    )

    results = result.to_json()["data"]["test_documents_halfvec"]

    assert len(results) == 3
    assert results[0]["title"] == "Java Tutorial"  # Closest to 0.5

@pytest.mark.asyncio
async def test_halfvec_memory_usage(db_pool, halfvec_test_setup):
    """Verify halfvec uses less memory than regular vector."""
    async with db_pool.connection() as conn:
        # Check storage size
        result = await conn.execute("""
            SELECT
                pg_column_size(embedding_half) as halfvec_size
            FROM test_documents_halfvec
            LIMIT 1
        """)
        row = await result.fetchone()

        # 384 dimensions * 2 bytes (16-bit) + header ~= 768-800 bytes
        # Regular vector would be 384 * 4 bytes = 1536 bytes
        assert row[0] < 850  # Should be roughly half the size
```

**Expected Failure**: Integration will fail because vector_type detection and plumbing not connected yet.

**Run Test:**
```bash
uv run pytest tests/integration/test_halfvec_e2e.py -xvs
# Expected: FAILED - Need to wire up vector_type detection
```

#### GREEN Phase

**Implementation:** Wire up vector_type detection in WHERE/ORDER BY generators

**File:** `src/fraiseql/sql/where/strategies/vector.py`

```python
from fraiseql.core.graphql_type import _detect_vector_type

class VectorComparisonStrategy(ComparisonStrategy):
    """Strategy for vector similarity operations."""

    def build_sql(
        self,
        field_name: str,
        operator: str,
        value: Any,
        table_ref: str = "data"
    ) -> Composed:
        """Build SQL for vector operations with type detection."""
        # Detect vector type from field name
        vector_type = _detect_vector_type(field_name) or "vector"

        path_sql = self._build_jsonb_path(field_name, table_ref)

        if operator == "cosine_distance":
            return build_cosine_distance_sql(path_sql, value, vector_type)
        elif operator == "l2_distance":
            return build_l2_distance_sql(path_sql, value, vector_type)
        elif operator == "inner_product":
            return build_inner_product_sql(path_sql, value, vector_type)
        elif operator == "l1_distance":
            return build_l1_distance_sql(path_sql, value, vector_type)
        else:
            raise ValueError(f"Unknown vector operator: {operator}")
```

**File:** `src/fraiseql/sql/order_by_generator.py`

```python
from fraiseql.core.graphql_type import _detect_vector_type

class OrderBy:
    """Order by instruction with vector type support."""

    def to_sql(self, table_ref: str = "t") -> Composed:
        """Generate ORDER BY SQL with vector type detection."""
        # ... existing code ...

        # For vector distance operations
        if "." in self.field and self.value is not None:
            parts = self.field.split(".")
            if len(parts) == 2:
                field_name, operator = parts

                # Detect vector type from field name
                vector_type = _detect_vector_type(field_name) or "vector"

                if operator in ("cosine_distance", "l2_distance", "inner_product", "l1_distance"):
                    return self._build_vector_distance_sql(
                        field_name,
                        operator,
                        self.value,
                        table_ref,
                        vector_type
                    )
```

**Run Test:**
```bash
uv run pytest tests/integration/test_halfvec_e2e.py -xvs
# Expected: PASSED
```

#### REFACTOR Phase

**Improvements:**
1. Cache vector type detection results
2. Add validation for halfvec dimension constraints
3. Improve error messages for type mismatches

#### QA Phase

**Verification:**
```bash
# Run all halfvec tests
uv run pytest tests/integration/test_halfvec_e2e.py -v

# Run all vector tests (ensure no regression)
uv run pytest tests/integration/test_vector_e2e.py -v

# Full test suite
uv run pytest tests/ -k vector --tb=short
```

**Success Criteria:**
- [ ] All halfvec tests pass
- [ ] No regression in existing vector tests
- [ ] Memory usage verified (50% reduction)
- [ ] Performance acceptable (similar to regular vectors)

---

### Phase 3.1 Summary

**Deliverables:**
- ✅ halfvec field detection by naming convention
- ✅ SQL generation for all 6 operators with halfvec
- ✅ WHERE clause support
- ✅ ORDER BY support
- ✅ Integration tests with real PostgreSQL
- ✅ Memory usage validation

**Time Spent:** 6-8 hours
**Tests Added:** ~15-20 tests
**Files Modified:** 6-8 files

---

## Phase 3.2: Sparse Vectors (`sparsevec`)

**Objective**: Add support for sparse vector representation
**Estimated Time**: 8-12 hours
**Complexity**: Medium-High
**Dependencies**: pgvector >= 0.5.0

### Background

PostgreSQL `sparsevec` type:
- Stores only non-zero values and their indices
- Format: `{1:0.5,3:0.8,7:0.3}/1536` (indices:values/dimensions)
- Memory efficient for high-dimensional sparse data
- Use cases: NLP (TF-IDF), sparse features, categorical embeddings

### TDD Cycle 3.2.1: sparsevec Data Format Handling

**Objective**: Convert Python sparse representations to PostgreSQL format

#### RED Phase

**Test:** `tests/unit/sql/test_sparsevec_conversion.py`

```python
import pytest
from fraiseql.sql.where.operators.vectors import (
    convert_to_sparsevec_format,
    build_cosine_distance_sql
)

def test_sparse_dict_to_sparsevec_format():
    """Test conversion from dict to sparsevec format."""
    # Python dict: {index: value}
    sparse_dict = {1: 0.5, 3: 0.8, 7: 0.3}
    dimensions = 10

    result = convert_to_sparsevec_format(sparse_dict, dimensions)

    # Should produce: {1:0.5,3:0.8,7:0.3}/10
    assert result == "{1:0.5,3:0.8,7:0.3}/10"

def test_sparse_list_to_sparsevec_format():
    """Test conversion from list of tuples to sparsevec format."""
    # List of (index, value) tuples
    sparse_list = [(0, 0.1), (5, 0.9), (9, 0.4)]
    dimensions = 10

    result = convert_to_sparsevec_format(sparse_list, dimensions)

    assert result == "{0:0.1,5:0.9,9:0.4}/10"

def test_dense_to_sparsevec_format():
    """Test automatic sparsification of dense vectors."""
    # Dense vector with many zeros
    dense_vector = [0.0, 0.5, 0.0, 0.8, 0.0, 0.0, 0.0, 0.3, 0.0, 0.0]

    result = convert_to_sparsevec_format(dense_vector)

    # Should extract non-zero indices
    assert result == "{1:0.5,3:0.8,7:0.3}/10"

def test_sparsevec_sql_generation():
    """Test SQL generation with sparsevec format."""
    from psycopg.sql import SQL, Identifier

    path_sql = SQL("t.").join([Identifier("sparse_features")])
    sparse_dict = {1: 0.5, 3: 0.8}

    sql = build_cosine_distance_sql(
        path_sql,
        sparse_dict,
        vector_type="sparsevec",
        dimensions=384
    )

    sql_string = sql.as_string(None)

    # Should generate: column::sparsevec <=> '{1:0.5,3:0.8}/384'::sparsevec
    assert "::sparsevec" in sql_string
    assert "{1:0.5,3:0.8}/384" in sql_string
```

**Expected Failure**: Functions don't exist yet.

#### GREEN Phase

**Implementation:** `src/fraiseql/sql/where/operators/vectors.py`

```python
def convert_to_sparsevec_format(
    sparse_data: dict[int, float] | list[tuple[int, float]] | list[float],
    dimensions: int | None = None
) -> str:
    """Convert Python sparse representations to PostgreSQL sparsevec format.

    Args:
        sparse_data: One of:
            - dict: {index: value} for sparse indices
            - list of tuples: [(index, value), ...]
            - list of floats: dense vector (auto-sparsify)
        dimensions: Total vector dimensions (inferred if not provided)

    Returns:
        PostgreSQL sparsevec format: "{1:0.5,3:0.8}/384"

    Examples:
        >>> convert_to_sparsevec_format({1: 0.5, 3: 0.8}, 10)
        '{1:0.5,3:0.8}/10'
    """
    # Handle dict format
    if isinstance(sparse_data, dict):
        if not dimensions:
            dimensions = max(sparse_data.keys()) + 1 if sparse_data else 1

        # Sort by index for consistent output
        items = sorted(sparse_data.items())
        sparse_str = ",".join(f"{idx}:{val}" for idx, val in items)
        return f"{{{sparse_str}}}/{dimensions}"

    # Handle list of tuples
    elif isinstance(sparse_data, list) and sparse_data and isinstance(sparse_data[0], tuple):
        if not dimensions:
            dimensions = max(idx for idx, _ in sparse_data) + 1

        items = sorted(sparse_data)
        sparse_str = ",".join(f"{idx}:{val}" for idx, val in items)
        return f"{{{sparse_str}}}/{dimensions}"

    # Handle dense vector (auto-sparsify)
    elif isinstance(sparse_data, list):
        dimensions = dimensions or len(sparse_data)

        # Extract non-zero values
        sparse_items = [(i, v) for i, v in enumerate(sparse_data) if v != 0.0]

        if not sparse_items:
            return f"{{}}/{dimensions}"  # All zeros

        sparse_str = ",".join(f"{idx}:{val}" for idx, val in sparse_items)
        return f"{{{sparse_str}}}/{dimensions}"

    else:
        raise ValueError(f"Unsupported sparse data format: {type(sparse_data)}")

def build_cosine_distance_sql(
    path_sql: SQL,
    value: list[float] | dict[int, float] | list[tuple[int, float]],
    vector_type: str = "vector",
    dimensions: int | None = None
) -> Composed:
    """Build SQL for cosine distance with support for sparse vectors."""

    if vector_type == "sparsevec":
        # Convert to sparsevec format
        vector_literal = convert_to_sparsevec_format(value, dimensions)
        cast_type = SQL("::sparsevec")
    else:
        # Regular vector format
        vector_literal = "[" + ",".join(str(v) for v in value) + "]"
        cast_type = SQL(f"::{vector_type}")

    return Composed([
        SQL("("),
        path_sql,
        SQL(")"),
        cast_type,
        SQL(" <=> "),
        Literal(vector_literal),
        cast_type
    ])
```

#### REFACTOR Phase

**Improvements:**
1. Add scipy.sparse support for scientific computing
2. Add validation for dimension consistency
3. Optimize sparse format generation

```python
def convert_to_sparsevec_format(
    sparse_data: dict[int, float] | list[tuple[int, float]] | list[float] | Any,
    dimensions: int | None = None
) -> str:
    """Convert Python sparse representations to PostgreSQL sparsevec format.

    Supports:
        - dict: {index: value}
        - list of tuples: [(index, value), ...]
        - list: dense vector (auto-sparsify)
        - scipy.sparse matrices (if scipy available)
    """
    # Try scipy sparse matrix support
    try:
        import scipy.sparse as sp
        if sp.issparse(sparse_data):
            # Convert to COO format for easy iteration
            coo = sparse_data.tocoo()
            sparse_dict = {int(idx): float(val) for idx, val in zip(coo.col, coo.data)}
            dimensions = dimensions or coo.shape[1]
            return convert_to_sparsevec_format(sparse_dict, dimensions)
    except ImportError:
        pass

    # ... rest of implementation ...
```

#### QA Phase

**Verification:**
```bash
uv run pytest tests/unit/sql/test_sparsevec_conversion.py -v
uv run pytest tests/unit/sql/test_sparsevec_operators.py -v
```

---

### TDD Cycle 3.2.2: sparsevec Integration Tests

**Objective**: End-to-end testing with real PostgreSQL

#### RED Phase

**Test:** `tests/integration/test_sparsevec_e2e.py`

```python
import pytest
import pytest_asyncio
import fraiseql as fraiseql_type
from fraiseql.db import FraiseQLRepository

@fraiseql_type
class DocumentSparse:
    """Document with sparse features."""
    id: int
    title: str
    sparse_features: dict[int, float]  # Detected as sparsevec

@pytest_asyncio.fixture
async def sparsevec_test_setup(db_pool):
    """Set up test table with sparsevec column."""
    async with db_pool.connection() as conn:
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS test_documents_sparse (
                id SERIAL PRIMARY KEY,
                title TEXT,
                sparse_features sparsevec(1536)
            )
        """)

        # Insert test data with sparse vectors
        await conn.execute("""
            INSERT INTO test_documents_sparse (title, sparse_features)
            VALUES
                ('Doc 1', '{1:0.5,100:0.8,500:0.3}/1536'::sparsevec),
                ('Doc 2', '{1:0.9,50:0.4,200:0.7}/1536'::sparsevec),
                ('Doc 3', '{10:0.6,100:0.2,300:0.9}/1536'::sparsevec)
        """)

        yield

        await conn.execute("DROP TABLE IF EXISTS test_documents_sparse CASCADE")

@pytest.mark.asyncio
async def test_sparsevec_cosine_distance_filter(db_pool, sparsevec_test_setup):
    """Test filtering with sparsevec cosine distance."""
    repo = FraiseQLRepository(db_pool)

    # Query with sparse vector (dict format)
    query_features = {1: 0.5, 100: 0.8}

    result = await repo.find(
        "test_documents_sparse",
        where={"sparse_features": {"cosine_distance": query_features}},
        limit=3
    )

    results = result.to_json()["data"]["test_documents_sparse"]

    assert len(results) == 3
    # Should find Doc 1 first (exact match on indices 1 and 100)
    assert results[0]["title"] == "Doc 1"

@pytest.mark.asyncio
async def test_sparsevec_memory_efficiency(db_pool, sparsevec_test_setup):
    """Verify sparsevec uses less memory than dense vectors."""
    async with db_pool.connection() as conn:
        result = await conn.execute("""
            SELECT pg_column_size(sparse_features) as sparse_size
            FROM test_documents_sparse
            LIMIT 1
        """)
        row = await result.fetchone()

        # Sparse vector with 3 non-zero values in 1536 dimensions
        # Should be much smaller than 1536 * 4 bytes = 6144 bytes
        assert row[0] < 100  # Should be tiny (only 3 values stored)
```

#### GREEN Phase

Implementation: Wire up sparsevec detection and conversion in repository layer.

#### REFACTOR Phase

Add automatic dimension inference and validation.

#### QA Phase

Full integration testing with various sparse formats.

---

### Phase 3.2 Summary

**Deliverables:**
- ✅ sparsevec format conversion (dict, list, scipy.sparse)
- ✅ SQL generation for all operators
- ✅ WHERE and ORDER BY support
- ✅ Memory efficiency validation
- ✅ Integration tests

**Time Spent:** 8-12 hours
**Tests Added:** ~20-25 tests

---

## Phase 3.3: Vector Aggregations

**Objective**: Add vector aggregation functions (AVG, SUM, centroid)
**Estimated Time**: 12-16 hours
**Complexity**: High
**Dependencies**: Extends FraiseQL's aggregation system

### Background

Vector aggregations enable:
- Cluster centroid calculation
- Batch similarity operations
- Vector statistics (mean, sum)
- GROUP BY with vector operations

PostgreSQL pgvector supports:
- `avg(vector_column)` - Average of vectors
- `sum(vector_column)` - Sum of vectors
- Compatible with GROUP BY

### TDD Cycle 3.3.1: Vector Aggregation Schema Generation

**Objective**: Auto-generate GraphQL aggregation types for vectors

#### RED Phase

**Test:** `tests/unit/core/test_vector_aggregation_schema.py`

```python
import pytest
import fraiseql as fraiseql_type

@fraiseql_type
class Product:
    id: int
    name: str
    embedding: list[float]

def test_vector_aggregation_type_generation():
    """Test that vector fields get aggregation functions."""
    # Should auto-generate ProductAggregations type
    assert hasattr(Product, "Aggregations")

    agg_fields = Product.Aggregations.__dataclass_fields__

    # Regular fields get count
    assert "count" in agg_fields

    # Vector fields should get avg and sum
    assert "embedding_avg" in agg_fields
    assert "embedding_sum" in agg_fields

def test_vector_aggregation_graphql_type():
    """Test GraphQL type generation for vector aggregations."""
    from fraiseql.core.graphql_type import _generate_aggregation_type

    agg_type = _generate_aggregation_type(Product)

    # Should generate GraphQL type with vector aggregations
    schema = agg_type._fraiseql_graphql_schema

    assert "embedding_avg: [Float!]" in schema
    assert "embedding_sum: [Float!]" in schema

def test_group_by_with_vector_aggregation():
    """Test GROUP BY queries with vector aggregations."""
    # Example: Group products by category, get avg embedding per category
    query = """
    query {
      products_grouped(
        groupBy: ["category"]
        aggregations: {
          count: true
          embedding_avg: true
        }
      ) {
        category
        count
        embedding_avg
      }
    }
    """
    # Should generate SQL:
    # SELECT category, COUNT(*), AVG(embedding)
    # FROM products
    # GROUP BY category
```

**Expected Failure**: Aggregation system doesn't support vector types yet.

#### GREEN Phase

**Implementation:** `src/fraiseql/core/aggregations.py` (new file)

```python
from dataclasses import dataclass, field
from typing import Any

VECTOR_AGGREGATION_FUNCTIONS = {
    "avg": "AVG",
    "sum": "SUM",
}

def _is_vector_field(field_type: type) -> bool:
    """Check if field is a vector type."""
    # Check for list[float] type hint
    if hasattr(field_type, "__origin__"):
        return (
            field_type.__origin__ is list
            and hasattr(field_type, "__args__")
            and field_type.__args__[0] is float
        )
    return False

def _generate_aggregation_type(source_type: type) -> type:
    """Generate aggregation type with vector support."""
    fields_dict = {}

    # Add count (always available)
    fields_dict["count"] = (int | None, None)

    # Add vector aggregations for vector fields
    for field_name, field_info in source_type.__dataclass_fields__.items():
        if _is_vector_field(field_info.type):
            # Add avg and sum for vector fields
            fields_dict[f"{field_name}_avg"] = (list[float] | None, None)
            fields_dict[f"{field_name}_sum"] = (list[float] | None, None)

    # Create dataclass dynamically
    agg_type_name = f"{source_type.__name__}Aggregations"
    agg_type = type(agg_type_name, (), fields_dict)

    return dataclass(agg_type)
```

**Implementation:** `src/fraiseql/sql/aggregation_generator.py` (new file)

```python
from psycopg.sql import SQL, Composed, Identifier

def build_vector_aggregation_sql(
    function: str,  # "avg" or "sum"
    field_name: str,
    table_ref: str = "data"
) -> Composed:
    """Build SQL for vector aggregation functions.

    Examples:
        AVG(data -> 'embedding')
        SUM(data -> 'features')
    """
    if function not in ("avg", "sum"):
        raise ValueError(f"Unsupported vector aggregation: {function}")

    return Composed([
        SQL(f"{function.upper()}("),
        SQL(f"{table_ref} -> "),
        SQL("'"),
        SQL(field_name),
        SQL("'"),
        SQL(")")
    ])
```

#### REFACTOR Phase

Integrate with existing FraiseQL aggregation system.

#### QA Phase

Test with various GROUP BY scenarios.

---

### TDD Cycle 3.3.2: Vector Aggregation Integration Tests

**Objective**: End-to-end testing with real aggregations

#### RED Phase

**Test:** `tests/integration/test_vector_aggregations.py`

```python
import pytest
import pytest_asyncio
import fraiseql as fraiseql_type
from fraiseql.db import FraiseQLRepository

@fraiseql_type
class ProductWithEmbedding:
    id: int
    category: str
    name: str
    embedding: list[float]

@pytest_asyncio.fixture
async def vector_agg_test_setup(db_pool):
    """Set up test data for aggregations."""
    async with db_pool.connection() as conn:
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS products_with_embedding (
                id SERIAL PRIMARY KEY,
                category TEXT,
                name TEXT,
                embedding vector(3)
            )
        """)

        await conn.execute("""
            INSERT INTO products_with_embedding (category, name, embedding)
            VALUES
                ('electronics', 'Phone', '[0.1, 0.2, 0.3]'),
                ('electronics', 'Laptop', '[0.2, 0.3, 0.4]'),
                ('books', 'Novel', '[0.5, 0.6, 0.7]'),
                ('books', 'Textbook', '[0.6, 0.7, 0.8]')
        """)

        yield

        await conn.execute("DROP TABLE IF EXISTS products_with_embedding CASCADE")

@pytest.mark.asyncio
async def test_vector_avg_aggregation(db_pool, vector_agg_test_setup):
    """Test AVG aggregation on vector column."""
    async with db_pool.connection() as conn:
        result = await conn.execute("""
            SELECT category, AVG(embedding)::text as avg_embedding
            FROM products_with_embedding
            GROUP BY category
            ORDER BY category
        """)

        rows = await result.fetchall()

        # Electronics: avg([0.1,0.2,0.3], [0.2,0.3,0.4]) = [0.15,0.25,0.35]
        assert rows[0][0] == "books"
        # Books: avg([0.5,0.6,0.7], [0.6,0.7,0.8]) = [0.55,0.65,0.75]
        assert rows[1][0] == "electronics"

@pytest.mark.asyncio
async def test_vector_aggregation_with_repository(db_pool, vector_agg_test_setup):
    """Test vector aggregations through FraiseQL repository."""
    repo = FraiseQLRepository(db_pool)

    # Query with aggregation
    result = await repo.aggregate(
        "products_with_embedding",
        group_by=["category"],
        aggregations={
            "count": True,
            "embedding_avg": True
        }
    )

    groups = result.to_json()["data"]["products_with_embedding_aggregated"]

    assert len(groups) == 2

    # Each group should have count and embedding_avg
    electronics = next(g for g in groups if g["category"] == "electronics")
    assert electronics["count"] == 2
    assert len(electronics["embedding_avg"]) == 3  # 3-dimensional vector

    # Verify avg calculation
    expected_avg = [0.15, 0.25, 0.35]  # avg of [0.1,0.2,0.3] and [0.2,0.3,0.4]
    for i, val in enumerate(electronics["embedding_avg"]):
        assert abs(val - expected_avg[i]) < 0.01
```

#### GREEN Phase

Implementation: Extend repository's `aggregate()` method to support vector aggregations.

#### REFACTOR Phase

Optimize SQL generation for complex aggregation queries.

#### QA Phase

Test with large datasets and multiple GROUP BY columns.

---

### Phase 3.3 Summary

**Deliverables:**
- ✅ Vector AVG and SUM aggregation functions
- ✅ GraphQL schema generation for aggregations
- ✅ GROUP BY support with vectors
- ✅ Integration with repository layer
- ✅ Comprehensive tests

**Time Spent:** 12-16 hours
**Tests Added:** ~25-30 tests

---

## Phase 3.4: Custom Distance Functions

**Objective**: API for user-defined distance metrics
**Estimated Time**: 10-14 hours
**Complexity**: High
**Dependencies**: PostgreSQL plpgsql or plpython3u

### Overview

Enable users to register custom distance functions for domain-specific similarity:
- Music similarity (weighted features)
- Chemical compound similarity
- Custom business logic
- Research and experimentation

### TDD Cycle 3.4.1: Custom Function Registration API

**Objective**: Design API for registering custom distance functions

#### RED Phase

**Test:** `tests/unit/core/test_custom_distance_api.py`

```python
import pytest
from fraiseql.vector import register_distance_function

def test_register_custom_distance_function():
    """Test registering a custom distance function."""

    @register_distance_function("weighted_cosine")
    def weighted_cosine_distance(
        vec1: list[float],
        vec2: list[float],
        weights: list[float]
    ) -> float:
        """Custom weighted cosine distance."""
        # Implementation doesn't matter for test
        pass

    # Should be registered in global registry
    from fraiseql.vector import CUSTOM_DISTANCE_FUNCTIONS
    assert "weighted_cosine" in CUSTOM_DISTANCE_FUNCTIONS

def test_custom_distance_sql_generation():
    """Test SQL generation for custom distance function."""
    from fraiseql.sql.where.operators.vectors import build_custom_distance_sql
    from psycopg.sql import SQL, Identifier

    path_sql = SQL("t.").join([Identifier("embedding")])
    query_vector = [0.1, 0.2, 0.3]
    weights = [1.0, 2.0, 1.5]

    sql = build_custom_distance_sql(
        path_sql,
        query_vector,
        function_name="weighted_cosine",
        params={"weights": weights}
    )

    sql_string = sql.as_string(None)

    # Should call custom function
    assert "weighted_cosine(" in sql_string

def test_custom_distance_graphql_integration():
    """Test that custom distances appear in GraphQL schema."""
    import fraiseql as fraiseql_type

    @fraiseql_type
    class Song:
        id: int
        title: str
        features: list[float]

    # Should auto-generate VectorFilter with custom distance
    filter_type = Song.VectorFilter

    # Should include custom distance operator
    assert hasattr(filter_type, "weighted_cosine")
```

**Expected Failure**: Custom function registration system doesn't exist.

#### GREEN Phase

**Implementation:** `src/fraiseql/vector/__init__.py` (new module)

```python
from typing import Callable, Any
from dataclasses import dataclass

@dataclass
class CustomDistanceFunction:
    """Metadata for custom distance function."""
    name: str
    python_func: Callable
    sql_template: str
    parameters: dict[str, type]

# Global registry
CUSTOM_DISTANCE_FUNCTIONS: dict[str, CustomDistanceFunction] = {}

def register_distance_function(
    name: str,
    sql_function: str | None = None
):
    """Decorator to register custom distance functions.

    Args:
        name: Function name (used in GraphQL)
        sql_function: PostgreSQL function name (if different from name)

    Example:
        @register_distance_function("weighted_cosine")
        def weighted_cosine(vec1, vec2, weights):
            '''Custom weighted cosine distance.'''
            pass
    """
    def decorator(func: Callable) -> Callable:
        # Extract parameter info from function signature
        import inspect
        sig = inspect.signature(func)
        params = {
            name: param.annotation
            for name, param in sig.parameters.items()
            if name not in ("vec1", "vec2")
        }

        custom_func = CustomDistanceFunction(
            name=name,
            python_func=func,
            sql_template=sql_function or name,
            parameters=params
        )

        CUSTOM_DISTANCE_FUNCTIONS[name] = custom_func

        return func

    return decorator
```

**Implementation:** `src/fraiseql/sql/where/operators/vectors.py`

```python
def build_custom_distance_sql(
    path_sql: SQL,
    value: list[float],
    function_name: str,
    params: dict[str, Any] | None = None
) -> Composed:
    """Build SQL for custom distance function.

    Generates: custom_function(column, query_vector, param1, param2, ...)
    """
    from fraiseql.vector import CUSTOM_DISTANCE_FUNCTIONS

    if function_name not in CUSTOM_DISTANCE_FUNCTIONS:
        raise ValueError(f"Unknown custom distance function: {function_name}")

    custom_func = CUSTOM_DISTANCE_FUNCTIONS[function_name]
    params = params or {}

    # Build function call
    parts = [
        SQL(f"{custom_func.sql_template}("),
        path_sql,
        SQL(", "),
        Literal("[" + ",".join(str(v) for v in value) + "]"),
    ]

    # Add custom parameters
    for param_name, param_value in params.items():
        parts.append(SQL(", "))
        parts.append(Literal(param_value))

    parts.append(SQL(")"))

    return Composed(parts)
```

#### REFACTOR Phase

Add validation, error handling, and SQL injection prevention.

#### QA Phase

Security testing for SQL injection in custom functions.

---

### TDD Cycle 3.4.2: PostgreSQL Function Creation

**Objective**: Auto-generate PostgreSQL functions from Python

#### Implementation

Create helper to generate plpgsql functions:

```python
def create_postgresql_function(
    conn,
    name: str,
    python_func: Callable,
    vector_dimensions: int = 384
):
    """Create PostgreSQL function from Python implementation.

    Note: Requires plpython3u extension.
    """
    # Generate plpgsql or plpython3u function
    # This is advanced - may need user to create functions manually
    pass
```

---

### Phase 3.4 Summary

**Deliverables:**
- ✅ Custom distance function registration API
- ✅ GraphQL schema generation for custom functions
- ✅ SQL generation with parameter passing
- ✅ Security validation
- ✅ Documentation and examples

**Time Spent:** 10-14 hours
**Tests Added:** ~15-20 tests

---

## Phase 3.5: Vector Quantization

**Objective**: Add product quantization and scalar quantization support
**Estimated Time**: 16-20 hours
**Complexity**: Very High
**Dependencies**: pgvector >= 0.7.0

### Background

Vector quantization compresses vectors for memory/performance optimization:

**Product Quantization (PQ):**
- Divides vectors into segments
- Quantizes each segment independently
- Significant memory reduction (8-16x)
- Slight accuracy loss

**Scalar Quantization (SQ):**
- Converts float32 to int8
- 4x memory reduction
- Faster comparisons
- Minimal accuracy loss

### TDD Cycle 3.5.1: Quantization Configuration

**Objective**: API for configuring quantization parameters

#### RED Phase

**Test:** `tests/unit/core/test_quantization_config.py`

```python
import pytest
from fraiseql.vector import QuantizationConfig, ProductQuantization, ScalarQuantization

def test_product_quantization_config():
    """Test product quantization configuration."""
    config = ProductQuantization(
        segments=8,  # Divide 384-dim vector into 8 segments of 48
        bits=8       # 8 bits per segment
    )

    assert config.segments == 8
    assert config.bits == 8
    assert config.compression_ratio == 16  # Roughly 16x compression

def test_scalar_quantization_config():
    """Test scalar quantization configuration."""
    config = ScalarQuantization(
        bits=8  # int8 quantization
    )

    assert config.bits == 8
    assert config.compression_ratio == 4  # 32-bit float -> 8-bit int

def test_quantization_index_creation():
    """Test index creation with quantization."""
    from fraiseql.vector import create_quantized_index

    # Should generate SQL for quantized index
    sql = create_quantized_index(
        table="documents",
        column="embedding",
        quantization=ProductQuantization(segments=8, bits=8)
    )

    # Should use ivfflat or hnsw with quantization
    assert "CREATE INDEX" in sql
    assert "ivfflat" in sql or "hnsw" in sql
```

**Expected Failure**: Quantization API doesn't exist.

#### GREEN Phase

**Implementation:** `src/fraiseql/vector/quantization.py` (new file)

```python
from dataclasses import dataclass
from enum import Enum

class QuantizationType(Enum):
    """Types of vector quantization."""
    PRODUCT = "product"
    SCALAR = "scalar"
    BINARY = "binary"

@dataclass
class ProductQuantization:
    """Product quantization configuration."""
    segments: int = 8
    bits: int = 8

    @property
    def compression_ratio(self) -> float:
        """Calculate compression ratio."""
        return 32 / self.bits  # Simplified calculation

@dataclass
class ScalarQuantization:
    """Scalar quantization configuration."""
    bits: int = 8

    @property
    def compression_ratio(self) -> float:
        return 32 / self.bits

def create_quantized_index(
    table: str,
    column: str,
    quantization: ProductQuantization | ScalarQuantization,
    index_type: str = "hnsw"
) -> str:
    """Generate SQL for creating quantized index.

    Args:
        table: Table name
        column: Vector column name
        quantization: Quantization configuration
        index_type: "hnsw" or "ivfflat"

    Returns:
        SQL CREATE INDEX statement
    """
    index_name = f"idx_{table}_{column}_quantized"

    if isinstance(quantization, ProductQuantization):
        # Product quantization requires special index parameters
        ops_class = f"vector_cosine_ops"  # Adjust based on distance metric
        with_params = f"WITH (m = 16, ef_construction = 64, quantization = 'pq{quantization.segments}x{quantization.bits}')"
    elif isinstance(quantization, ScalarQuantization):
        ops_class = f"vector_cosine_ops"
        with_params = f"WITH (quantization = 'sq{quantization.bits}')"
    else:
        raise ValueError(f"Unknown quantization type: {type(quantization)}")

    return f"""
    CREATE INDEX {index_name}
    ON {table}
    USING {index_type} ({column} {ops_class})
    {with_params};
    """
```

#### REFACTOR Phase

Add validation, parameter optimization, and benchmarking tools.

#### QA Phase

Performance benchmarking: measure compression ratio, query speed, and accuracy.

---

### TDD Cycle 3.5.2: Quantization Integration Tests

**Objective**: Test quantization with real PostgreSQL

#### RED Phase

**Test:** `tests/integration/test_quantization.py`

```python
import pytest
import pytest_asyncio
from fraiseql.vector import ProductQuantization, create_quantized_index

@pytest_asyncio.fixture
async def quantization_test_setup(db_pool):
    """Set up test table with large vector dataset."""
    async with db_pool.connection() as conn:
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS documents_large (
                id SERIAL PRIMARY KEY,
                title TEXT,
                embedding vector(384)
            )
        """)

        # Insert 10,000 test vectors
        import numpy as np
        for i in range(10000):
            vec = np.random.rand(384).tolist()
            vec_str = "[" + ",".join(str(v) for v in vec) + "]"
            await conn.execute(
                f"INSERT INTO documents_large (title, embedding) VALUES ($1, $2)",
                f"Document {i}",
                vec_str
            )

        yield

        await conn.execute("DROP TABLE IF EXISTS documents_large CASCADE")

@pytest.mark.asyncio
async def test_product_quantization_index(db_pool, quantization_test_setup):
    """Test creating and using product quantized index."""
    async with db_pool.connection() as conn:
        # Create quantized index
        pq_config = ProductQuantization(segments=8, bits=8)
        index_sql = create_quantized_index(
            "documents_large",
            "embedding",
            pq_config
        )

        await conn.execute(index_sql)

        # Query with quantized index
        query_vec = "[" + ",".join("0.5" for _ in range(384)) + "]"
        result = await conn.execute(f"""
            SELECT title, embedding <=> '{query_vec}'::vector as distance
            FROM documents_large
            ORDER BY embedding <=> '{query_vec}'::vector
            LIMIT 10
        """)

        rows = await result.fetchall()
        assert len(rows) == 10

        # Verify index is being used
        explain_result = await conn.execute(f"""
            EXPLAIN SELECT title
            FROM documents_large
            ORDER BY embedding <=> '{query_vec}'::vector
            LIMIT 10
        """)

        explain_text = await explain_result.fetchall()
        # Should use index
        assert any("Index Scan" in str(row) for row in explain_text)

@pytest.mark.asyncio
async def test_quantization_memory_savings(db_pool, quantization_test_setup):
    """Measure memory savings from quantization."""
    async with db_pool.connection() as conn:
        # Check table size before quantization
        result_before = await conn.execute("""
            SELECT pg_total_relation_size('documents_large') as size_bytes
        """)
        size_before = (await result_before.fetchone())[0]

        # Create quantized index
        pq_config = ProductQuantization(segments=8, bits=8)
        index_sql = create_quantized_index(
            "documents_large",
            "embedding",
            pq_config
        )
        await conn.execute(index_sql)

        # Check index size
        result_index = await conn.execute("""
            SELECT pg_relation_size('idx_documents_large_embedding_quantized') as size_bytes
        """)
        index_size = (await result_index.fetchone())[0]

        # Quantized index should be much smaller than original data
        # 384 dims * 4 bytes = 1536 bytes per vector
        # PQ 8x8: 384/8 segments * 1 byte = 48 bytes per vector
        # ~32x compression
        expected_max_size = size_before / 16  # At least 16x compression
        assert index_size < expected_max_size

@pytest.mark.asyncio
async def test_quantization_accuracy_tradeoff(db_pool, quantization_test_setup):
    """Test accuracy vs compression tradeoff."""
    async with db_pool.connection() as conn:
        query_vec = "[" + ",".join("0.5" for _ in range(384)) + "]"

        # Get results without quantization
        result_exact = await conn.execute(f"""
            SELECT id, embedding <=> '{query_vec}'::vector as distance
            FROM documents_large
            ORDER BY distance
            LIMIT 100
        """)
        exact_results = await result_exact.fetchall()

        # Create quantized index and query
        pq_config = ProductQuantization(segments=8, bits=8)
        index_sql = create_quantized_index(
            "documents_large",
            "embedding",
            pq_config
        )
        await conn.execute(index_sql)

        result_quantized = await conn.execute(f"""
            SELECT id, embedding <=> '{query_vec}'::vector as distance
            FROM documents_large
            ORDER BY distance
            LIMIT 100
        """)
        quantized_results = await result_quantized.fetchall()

        # Calculate recall@100
        exact_ids = {row[0] for row in exact_results}
        quantized_ids = {row[0] for row in quantized_results}

        recall = len(exact_ids & quantized_ids) / len(exact_ids)

        # Should have >90% recall even with quantization
        assert recall > 0.90
```

#### GREEN Phase

Implementation: Integrate quantization with repository and index management.

#### REFACTOR Phase

Add automatic parameter tuning and optimization.

#### QA Phase

Extensive performance benchmarking with various configurations.

---

### Phase 3.5 Summary

**Deliverables:**
- ✅ Product quantization configuration and indexing
- ✅ Scalar quantization support
- ✅ Memory usage measurement and validation
- ✅ Accuracy/compression tradeoff analysis
- ✅ Performance benchmarks
- ✅ Documentation with best practices

**Time Spent:** 16-20 hours
**Tests Added:** ~20-25 tests

---

## Phase 3 Complete: Success Criteria

### Technical Deliverables

- [ ] All 5 features implemented and tested
- [ ] 100+ new tests added (>95% coverage)
- [ ] All tests passing in CI
- [ ] Performance benchmarks published
- [ ] Zero regressions in existing functionality

### Documentation Deliverables

- [ ] Feature documentation for each capability
- [ ] Migration guides from competitors
- [ ] Performance tuning guides
- [ ] Best practices documentation
- [ ] Example applications

### Code Quality

- [ ] Type hints on all new code
- [ ] Docstrings with examples
- [ ] Linting passes (ruff)
- [ ] Type checking passes (mypy)
- [ ] Security review complete

## Timeline Summary

| Phase | Feature | Time Estimate |
|-------|---------|---------------|
| 3.1 | Half-precision vectors | 6-8 hours |
| 3.2 | Sparse vectors | 8-12 hours |
| 3.3 | Vector aggregations | 12-16 hours |
| 3.4 | Custom distance functions | 10-14 hours |
| 3.5 | Vector quantization | 16-20 hours |
| **Total** | **Phase 3 Complete** | **52-70 hours** |

**Realistic Estimate with Buffer:** 61 hours (~8 working days)

## Post-Phase 3: Market Position

After completing Phase 3, FraiseQL will be:

🥇 **#1 GraphQL framework for AI/ML applications**
- Only framework with complete pgvector feature parity
- 6-12 months ahead of any competitors
- Production-ready for enterprise vector workloads

🏆 **Unique Market Position:**
- Python-native + GraphQL + Complete Vector Search
- No other framework offers this combination
- Defensible technical moat

💰 **$200B+ Market Opportunity:**
- AI/ML applications
- RAG systems
- Semantic search
- Recommendation engines
- Enterprise data analytics

## Next Steps After Phase 3

1. **LangChain Integration** (20 hours) - Become standard for RAG
2. **Performance Benchmarks** (15 hours) - Prove production-readiness
3. **Developer Experience** (30 hours) - Polish and tutorials
4. **Enterprise Features** (40 hours) - Multi-tenancy, monitoring

**Total to Market Leadership:** ~136 hours (~3-4 weeks)

---

## Appendix: Testing Strategy

### Unit Tests
- Field detection (vector type identification)
- SQL generation (all operators, all types)
- Format conversion (sparse, quantization)
- Schema generation (GraphQL types)

### Integration Tests
- Real PostgreSQL with pgvector
- All vector types (vector, halfvec, sparsevec)
- All operators (6 distance functions)
- Performance validation
- Memory usage verification

### End-to-End Tests
- Full GraphQL queries
- Repository operations
- Aggregation queries
- Quantized index operations

### Performance Tests
- Query speed benchmarks
- Memory usage measurement
- Compression ratio validation
- Accuracy/recall metrics

## Appendix: Risk Mitigation

### Technical Risks

**Risk:** pgvector version compatibility
**Mitigation:** Test against multiple pgvector versions, document requirements

**Risk:** Performance degradation
**Mitigation:** Comprehensive benchmarks, optimization phase in each cycle

**Risk:** Memory usage issues
**Mitigation:** Explicit memory tests, quantization validation

**Risk:** Security vulnerabilities (custom functions)
**Mitigation:** SQL injection prevention, parameter validation, security review

### Project Risks

**Risk:** Scope creep
**Mitigation:** Strict phase boundaries, clear success criteria

**Risk:** Timeline overrun
**Mitigation:** Realistic estimates with buffer, phased approach allows early stopping

**Risk:** Breaking changes
**Mitigation:** Comprehensive backward compatibility tests, semantic versioning

## Appendix: Resources

### Documentation to Create

1. Feature guides for each capability
2. Migration guides (from Pinecone, Weaviate, etc.)
3. Performance tuning documentation
4. Best practices guide
5. Example applications (RAG, semantic search, recommendations)

### Example Applications to Build

1. Semantic document search
2. RAG chatbot backend
3. Product recommendation engine
4. Image similarity search
5. Customer clustering analysis

### Benchmarks to Publish

1. FraiseQL vs Pinecone (cost, performance)
2. FraiseQL vs custom Apollo + pgvector
3. Quantization accuracy/speed tradeoffs
4. Memory usage comparisons
5. Query performance at scale (1M, 10M, 100M vectors)

---

**End of Phase 3 Implementation Plan**

This plan represents ~61 hours of focused development work to establish FraiseQL as the leading GraphQL framework for AI/ML applications with complete pgvector feature parity.
