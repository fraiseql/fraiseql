# Implementation Plan: PostgreSQL pgvector Support for FraiseQL

**Issue**: #134
**Status**: Planning
**Complexity**: Complex - Multi-file architecture change requiring phased TDD approach

---

## Executive Summary

Add native PostgreSQL pgvector support to FraiseQL, enabling vector similarity search through GraphQL filters. This exposes pgvector's three distance operators (`<=>`, `<->`, `<#>`) through type-safe GraphQL interfaces, allowing semantic search, recommendations, and RAG system integration without additional infrastructure.

**Philosophy**: Expose native PostgreSQL capabilities with minimal abstraction—FraiseQL as a thin, transparent layer over pgvector.

---

## Design Decisions (FraiseQL Philosophy Applied)

### 1. Operator Naming: PostgreSQL Terms ✓

**Decision**: Use `cosine_distance`, `l2_distance`, `inner_product` (PostgreSQL native terms)

**Rationale**:
- FraiseQL is a thin, transparent layer over PostgreSQL
- Existing operators use PostgreSQL terminology (`ancestor_of`, `matches_lquery`, `strictly_left`)
- Users expect PostgreSQL semantics, not ML abstractions
- Avoids confusion: operators return distances (lower = more similar)

```python
class VectorFilter:
    cosine_distance: list[float] | None = None  # <=> operator
    l2_distance: list[float] | None = None      # <-> operator
    inner_product: list[float] | None = None    # <#> operator
    isnull: bool | None = None
```

### 2. Distance vs Similarity: Expose Raw PostgreSQL Distances ✓

**Decision**: Return raw distances from PostgreSQL, no conversion to similarities

**Rationale**:
- FraiseQL never transforms PostgreSQL return values
- PostgreSQL pgvector returns distances natively
- Converting would add abstraction (anti-pattern for FraiseQL)
- Users can convert in application code if needed

**Distance semantics** (document in VectorFilter docstring):
- `cosine_distance`: 0.0 = identical, 2.0 = opposite
- `l2_distance`: 0.0 = identical, ∞ = very different
- `inner_product`: More negative = more similar

**OrderBy behavior**:
```graphql
orderBy: { embedding: { cosine_distance: [...] } }  # ASC = most similar first
```

### 3. Dimension Validation: Let PostgreSQL Handle It ✓

**Decision**: No dimension validation in FraiseQL

**Rationale**:
- FraiseQL pattern: minimal validation, trust PostgreSQL
- Vector dimensions are table-specific (`vector(384)`, `vector(1536)`, etc.)
- FraiseQL has no knowledge of target column dimensions at filter time
- PostgreSQL returns clear errors: `ERROR: different vector dimensions 384 and 1536`
- Avoids maintaining dimension metadata

**Validation approach** (basic type checking only):
```python
@staticmethod
def parse_value(value: list[float]) -> list[float]:
    if not isinstance(value, list):
        raise ValueError("Vector must be a list of floats")
    if not all(isinstance(x, (int, float)) for x in value):
        raise ValueError("All vector values must be numbers")
    # NO dimension validation - let PostgreSQL handle it
    return value
```

### 4. Index Hints: No Warnings or Hints ✓

**Decision**: No index warnings at runtime

**Rationale**:
- FraiseQL doesn't warn about missing indexes (not its responsibility)
- PostgreSQL handles query planning and index usage
- Existing specialized types don't warn (IP GiST, ltree GiST, tsvector GIN)
- Separation of concerns: users handle database optimization
- Would require introspecting `pg_indexes` (performance overhead)

**Approach**: Document HNSW index best practices in examples/docs.

### 5. Array vs Vector Disambiguation: Field Name Pattern Detection ✓

**Decision**: Use field name patterns (same as IP, MAC, ltree, tsvector)

**Rationale**:
- Python type hints alone insufficient (`list[float]` ambiguous)
- FraiseQL already uses field name patterns extensively
- Common ML/AI naming conventions exist

**Detection patterns**:
```python
# In _detect_field_type_from_name()
vector_patterns = [
    "embedding",
    "vector",
    "_embedding",
    "_vector",
    "embedding_vector",
    "text_embedding",
    "image_embedding",
]
```

**Priority order**:
1. Explicit type hint (if `Vector` type class created)
2. **Field name patterns** (check before generic value analysis)
3. Value analysis (`list[float]` defaults to ARRAY if no pattern match)

**Example**:
```python
@type(sql_source="v_document")
class Document:
    id: UUID
    tags: list[str]              # → ArrayFilter (no vector pattern)
    scores: list[float]           # → ArrayFilter (no vector pattern)
    embedding: list[float]        # → VectorFilter (matches "embedding")
    text_embedding: list[float]   # → VectorFilter (matches pattern)
```

---

## PHASE 1: Core Vector Field Type Infrastructure

**Objective**: Establish vector field type detection and basic type system support

### TDD Cycle 1.1: Add VECTOR FieldType

**RED**: Write failing test for vector field type detection
- Test file: `tests/unit/sql/where/core/test_field_detection.py`
- Add test cases for:
  - `test_detect_vector_from_field_name_embedding_suffix()` - `embedding`, `text_embedding`
  - `test_detect_vector_from_field_name_vector_suffix()` - `_vector`, `embedding_vector`
  - `test_vector_vs_array_disambiguation()` - field name patterns distinguish types
  - `test_vector_field_type_enum_exists()` - VECTOR enum exists

**GREEN**: Implement minimal code
- File: `src/fraiseql/sql/where/core/field_detection.py`
  - Add `VECTOR = "vector"` to `FieldType` enum (after line 32)
  - Add vector pattern detection in `_detect_field_type_from_name()` (before line 437):
    ```python
    # Vector embedding patterns - handle both snake_case and camelCase
    vector_patterns = [
        "embedding",
        "vector",
        "_embedding",
        "_vector",
        "embedding_vector",
        "embeddingvector",
        "text_embedding",
        "textembedding",
        "image_embedding",
        "imageembedding",
    ]

    # Check vector pattern matches
    if any(pattern in field_lower for pattern in vector_patterns):
        return FieldType.VECTOR
    ```
  - Note: Add BEFORE ARRAY detection to take precedence for `list[float]` with vector names

**REFACTOR**: Clean up detection logic
- Ensure vector detection doesn't conflict with existing ARRAY type
- Position vector detection to have correct precedence
- Add comprehensive field name patterns following existing conventions

**QA**: Verify phase completion
- [ ] All unit tests pass
- [ ] Vector fields detected correctly by name pattern
- [ ] Regular `list[float]` fields still detected as ARRAY
- [ ] No regression in existing field type detection

---

## PHASE 2: PostgreSQL Vector Operators

**Objective**: Implement SQL generation for pgvector's three native distance operators

### TDD Cycle 2.1: Vector Distance Operators

**RED**: Write failing tests for vector SQL generation
- Test file: `tests/unit/sql/where/operators/test_vectors.py` (new file)
- Test cases:
  - `test_cosine_distance_sql()` - generates `column <=> '[0.1,0.2,...]'::vector`
  - `test_l2_distance_sql()` - generates `column <-> '[0.1,0.2,...]'::vector`
  - `test_inner_product_sql()` - generates `column <#> '[0.1,0.2,...]'::vector`
  - `test_vector_casting_format()` - proper PostgreSQL array literal format
  - `test_vector_null_handling()` - NULL vectors handled correctly

**GREEN**: Implement vector operators
- File: `src/fraiseql/sql/where/operators/vectors.py` (new file)
  - Follow pattern from `network.py` for proper type casting
  - Implement three pgvector operators:
    ```python
    """Vector/embedding specific operators for PostgreSQL pgvector.

    This module exposes PostgreSQL's native pgvector distance operators:
    - <=> : cosine distance (0.0 = identical, 2.0 = opposite)
    - <-> : L2/Euclidean distance (0.0 = identical, ∞ = very different)
    - <#> : negative inner product (more negative = more similar)

    FraiseQL exposes these operators transparently without abstraction.
    Distance values are returned raw from PostgreSQL (no conversion to similarity).
    """

    from psycopg.sql import SQL, Composed, Literal

    def build_cosine_distance_sql(path_sql: SQL, value: list[float]) -> Composed:
        """Build SQL for cosine distance using PostgreSQL <=> operator.

        Generates: column <=> '[0.1,0.2,...]'::vector
        Returns distance: 0.0 (identical) to 2.0 (opposite)
        """
        vector_literal = "[" + ",".join(str(v) for v in value) + "]"
        return Composed([
            SQL("("), path_sql, SQL(")::vector <=> "),
            Literal(vector_literal), SQL("::vector")
        ])

    def build_l2_distance_sql(path_sql: SQL, value: list[float]) -> Composed:
        """Build SQL for L2/Euclidean distance using PostgreSQL <-> operator.

        Generates: column <-> '[0.1,0.2,...]'::vector
        Returns distance: 0.0 (identical) to ∞ (very different)
        """
        vector_literal = "[" + ",".join(str(v) for v in value) + "]"
        return Composed([
            SQL("("), path_sql, SQL(")::vector <-> "),
            Literal(vector_literal), SQL("::vector")
        ])

    def build_inner_product_sql(path_sql: SQL, value: list[float]) -> Composed:
        """Build SQL for inner product using PostgreSQL <#> operator.

        Generates: column <#> '[0.1,0.2,...]'::vector
        Returns negative inner product: more negative = more similar
        """
        vector_literal = "[" + ",".join(str(v) for v in value) + "]"
        return Composed([
            SQL("("), path_sql, SQL(")::vector <#> "),
            Literal(vector_literal), SQL("::vector")
        ])
    ```

**REFACTOR**: Optimize SQL generation
- Extract vector literal formatting to helper function
- Ensure proper psycopg.sql composition
- Add comprehensive docstrings explaining pgvector operators and distance semantics

**QA**: Verify operator implementation
- [ ] SQL generated matches PostgreSQL pgvector syntax exactly
- [ ] Vector values properly formatted as PostgreSQL array literals
- [ ] Type casting to `::vector` applied correctly
- [ ] Integration with existing operator system works

### TDD Cycle 2.2: Register Vector Operators

**RED**: Write test for operator registration
- Test file: `tests/unit/sql/where/operators/test_operator_map.py`
- Test cases:
  - `test_vector_operators_registered()` - all three operators in map
  - `test_get_operator_function_vector()` - `get_operator_function()` returns builders
  - `test_vector_operator_function_signatures()` - correct signatures

**GREEN**: Register in OPERATOR_MAP
- File: `src/fraiseql/sql/where/operators/__init__.py`
  - Import vectors module (add to imports around line 13-30):
    ```python
    from . import (
        arrays,
        basic,
        date,
        date_range,
        datetime,
        email,
        fulltext,
        hostname,
        jsonb,
        lists,
        ltree,
        mac_address,
        network,
        nulls,
        port,
        text,
        vectors,  # ADD THIS
    )
    ```
  - Add mappings to OPERATOR_MAP (after line 201):
    ```python
    # Vector operators for PostgreSQL pgvector distance operations
    (FieldType.VECTOR, "cosine_distance"): vectors.build_cosine_distance_sql,
    (FieldType.VECTOR, "l2_distance"): vectors.build_l2_distance_sql,
    (FieldType.VECTOR, "inner_product"): vectors.build_inner_product_sql,
    ```

**REFACTOR**: Clean up operator map
- Group vector operators with other specialized PostgreSQL types (near network, ltree)
- Add clear comments explaining pgvector operators

**QA**: Verify operator registration
- [ ] `get_operator_function()` returns correct builder for vector operators
- [ ] No conflicts with existing operators
- [ ] All imports resolve correctly

---

## PHASE 3: GraphQL Schema Generation

**Objective**: Generate GraphQL VectorFilter input type for schema

### TDD Cycle 3.1: VectorFilter Type Definition

**RED**: Write failing test for VectorFilter schema
- Test file: `tests/integration/graphql/schema/test_vector_filter.py` (new file)
- Test cases:
  - `test_vector_filter_in_schema()` - VectorFilter type exists in schema
  - `test_vector_filter_fields()` - has cosine_distance, l2_distance, inner_product
  - `test_vector_filter_field_types()` - fields are `[Float!]`
  - `test_vector_filter_docstring()` - proper GraphQL documentation

**GREEN**: Implement VectorFilter
- File: `src/fraiseql/sql/graphql_where_generator.py`
  - Add `VectorFilter` class (after line 334, following JSONBFilter pattern):
    ```python
    @fraise_input
    class VectorFilter:
        """PostgreSQL pgvector field filter operations.

        Exposes native pgvector distance operators transparently:
        - cosine_distance: Cosine distance (0.0 = identical, 2.0 = opposite)
        - l2_distance: L2/Euclidean distance (0.0 = identical, ∞ = different)
        - inner_product: Negative inner product (more negative = more similar)

        Distance values are returned raw from PostgreSQL (no conversion).
        Requires pgvector extension: CREATE EXTENSION vector;

        Example:
            documents(
                where: { embedding: { cosine_distance: [0.1, 0.2, ...] } }
                orderBy: { embedding: { cosine_distance: [0.1, 0.2, ...] } }
                limit: 10
            )
        """
        cosine_distance: list[float] | None = None
        l2_distance: list[float] | None = None
        inner_product: list[float] | None = None
        isnull: bool | None = None
    ```

**REFACTOR**: Clean up filter definition
- Add comprehensive docstrings explaining pgvector operators and semantics
- Ensure consistent naming and structure with other filters
- Document distance return values (not similarities)

**QA**: Verify filter type
- [ ] VectorFilter generates correct GraphQL schema
- [ ] Operators match pgvector capabilities exactly
- [ ] Documentation clear about distance semantics

### TDD Cycle 3.2: Vector Type Mapping

**RED**: Write test for vector type detection in GraphQL
- Test file: `tests/integration/graphql/schema/test_filter_type_mapping.py`
- Test cases:
  - `test_embedding_field_maps_to_vector_filter()` - field named `embedding`
  - `test_text_embedding_maps_to_vector_filter()` - field named `text_embedding`
  - `test_regular_list_float_maps_to_array_filter()` - field named `scores`
  - `test_vector_pattern_precedence()` - vector detection happens before array

**GREEN**: Update type mapping
- File: `src/fraiseql/sql/graphql_where_generator.py`
  - Update `_get_filter_type_for_field()` (around line 370-377, BEFORE list detection):
    ```python
    # Check for vector/embedding fields by name pattern (BEFORE list detection)
    # This allows list[float] to map to VectorFilter for embeddings
    if field_name:
        field_lower = field_name.lower()
        vector_patterns = [
            "embedding",
            "vector",
            "_embedding",
            "_vector",
            "embedding_vector",
            "embeddingvector",
            "text_embedding",
            "textembedding",
            "image_embedding",
            "imageembedding",
        ]
        if any(pattern in field_lower for pattern in vector_patterns):
            # Check if it's actually a list type
            origin = get_origin(field_type)
            if origin is list:
                return VectorFilter

    # List type detection (existing code around line 374)
    if get_origin(field_type) is list:
        return ArrayFilter
    ```

**REFACTOR**: Improve type detection
- Ensure vector detection has correct precedence (before generic list detection)
- Balance between ARRAY and VECTOR type detection using field name heuristics
- Add comments explaining disambiguation logic

**QA**: Verify type mapping
- [ ] Vector fields (by name pattern) get VectorFilter in schema
- [ ] Regular list fields still get ArrayFilter
- [ ] `list[float]` with vector name patterns → VectorFilter
- [ ] `list[float]` without vector patterns → ArrayFilter
- [ ] No regression in existing type mappings

---

## PHASE 4: Vector Value Handling & Validation

**Objective**: Proper serialization and validation of vector values (minimal validation per FraiseQL philosophy)

### TDD Cycle 4.1: Vector Value Validation

**RED**: Write failing test for vector value handling
- Test file: `tests/unit/types/test_vector_validation.py` (new file)
- Test cases:
  - `test_vector_accepts_list_of_floats()` - valid vectors pass
  - `test_vector_accepts_list_of_ints()` - integers coerced to floats
  - `test_vector_rejects_non_list()` - strings, dicts rejected
  - `test_vector_rejects_non_numeric()` - lists with strings rejected
  - `test_vector_no_dimension_validation()` - any dimension accepted

**GREEN**: Add basic validation to VectorFilter
- File: `src/fraiseql/sql/graphql_where_generator.py`
  - Add validation in VectorFilter field annotations (if needed by Strawberry)
  - OR rely on Strawberry's `list[float]` type checking (preferred)
  - Optional: Create custom scalar if more control needed

**Alternative approach** (if custom scalar needed):
- File: `src/fraiseql/types/scalars/vector.py` (new file, optional)
  ```python
  """Vector scalar type for PostgreSQL pgvector.

  Minimal validation following FraiseQL philosophy:
  - Verify value is list of numbers
  - Let PostgreSQL handle dimension validation
  - No conversion or transformation
  """

  import strawberry

  @strawberry.scalar(
      description="PostgreSQL vector type (list of floats for embeddings)"
  )
  class Vector:
      @staticmethod
      def serialize(value: list[float]) -> list[float]:
          """Serialize vector to GraphQL output (no transformation)."""
          return value

      @staticmethod
      def parse_value(value: list[float]) -> list[float]:
          """Parse GraphQL input to vector (basic validation only)."""
          if not isinstance(value, list):
              raise ValueError("Vector must be a list of floats")
          if not all(isinstance(x, (int, float)) for x in value):
              raise ValueError("All vector values must be numbers")
          # NO dimension validation - let PostgreSQL handle it
          return [float(x) for x in value]  # Coerce ints to floats
  ```

**REFACTOR**: Optimize validation
- Minimal validation in FraiseQL (trust PostgreSQL per philosophy)
- Clear error messages for invalid input (wrong type)
- Document that dimension validation happens in PostgreSQL

**QA**: Verify value handling
- [ ] Vector values properly serialized to PostgreSQL
- [ ] Invalid vectors (non-numeric) rejected with clear errors
- [ ] Dimension mismatches caught by PostgreSQL (not FraiseQL)
- [ ] Performance acceptable for large vectors (up to 2000 dimensions)

---

## PHASE 5: OrderBy Vector Distance Support

**Objective**: Enable ordering query results by vector distance

### TDD Cycle 5.1: Locate ORDER BY Generation Logic

**RED**: Write failing test for ORDER BY vector distance
- Test file: `tests/unit/sql/test_order_by_vector.py` (new file)
- Test cases:
  - `test_order_by_cosine_distance()` - generates `ORDER BY column <=> '[...]'::vector`
  - `test_order_by_l2_distance()` - generates `ORDER BY column <-> '[...]'::vector`
  - `test_order_by_inner_product()` - generates `ORDER BY column <#> '[...]'::vector`
  - `test_order_by_vector_asc_default()` - ASC is default (most similar first)

**GREEN**: Investigate and implement ORDER BY support
- Task: Use Explore agent to find ORDER BY generation code
  - Likely in query builder or schema generation
  - Look for `orderBy` parameter handling
  - Check how other field types handle complex ORDER BY (e.g., full-text rank)
- Implement vector distance operator support for ordering
- Generate SQL: `ORDER BY embedding <=> '[0.1,0.2,...]'::vector ASC`

**REFACTOR**: Clean up ordering logic
- Ensure consistent with other ORDER BY operators
- Handle ASC/DESC properly (ASC = most similar first for distances)
- Reuse vector operator SQL builders from Phase 2

**QA**: Verify ordering
- [ ] ORDER BY with vector distance generates correct SQL
- [ ] Reuses operator builders (DRY principle)
- [ ] ASC/DESC work correctly
- [ ] Integration with existing query system works

---

## PHASE 6: Integration Testing & Documentation

**Objective**: End-to-end testing and user documentation

### TDD Cycle 6.1: Integration Tests

**RED**: Write failing E2E tests
- Test file: `tests/integration/test_vector_e2e.py` (new file)
- Test complete flow with real PostgreSQL + pgvector:
  - `test_vector_filter_cosine_distance()` - filter by cosine distance
  - `test_vector_order_by_distance()` - order by similarity
  - `test_vector_with_other_filters()` - compose with tenant_id, timestamps
  - `test_vector_limit_results()` - pagination works
  - `test_vector_dimension_mismatch_error()` - PostgreSQL error handling
  - `test_vector_hnsw_index_performance()` - verify index usage (optional)

**GREEN**: Ensure all integration works
- Set up test PostgreSQL database with pgvector extension
- Create test tables with vector columns and HNSW indexes
- Fix any issues discovered in E2E testing
- Verify PostgreSQL view pattern works correctly

**REFACTOR**: Optimize integration
- Performance testing with actual pgvector indexes
- Ensure Rust pipeline compatibility (if applicable)
- Clean up test fixtures

**QA**: Verify complete feature
- [ ] E2E tests pass with real PostgreSQL + pgvector
- [ ] Works with HNSW and IVFFlat indexes
- [ ] Performance acceptable (index usage verified)
- [ ] Composes correctly with existing filters
- [ ] No regressions in existing functionality

### TDD Cycle 6.2: Documentation

**RED**: Documentation checklist
- [ ] Feature guide in `docs/features/pgvector.md`
- [ ] Example in `docs/examples/semantic-search.md`
- [ ] Migration guide section
- [ ] API reference for VectorFilter
- [ ] README section mentioning vector support

**GREEN**: Write comprehensive documentation
- File: `docs/features/pgvector.md` (new file)
  - PostgreSQL setup (CREATE EXTENSION vector)
  - Creating vector columns and indexes
  - FraiseQL type definition with vector fields
  - GraphQL query examples (filter, orderBy)
  - Distance semantics explanation
  - Performance tips (HNSW vs IVFFlat indexes)

- File: `docs/examples/semantic-search.md` (new file)
  - Complete semantic search example
  - Document embedding generation (external, not FraiseQL's job)
  - RAG system pattern
  - Hybrid search (full-text + vector)

- File: `README.md` - Add vector support to features list

**REFACTOR**: Improve documentation
- Add code examples that users can copy-paste
- Link to pgvector official documentation
- Include performance benchmarks (optional)
- Add troubleshooting section

**QA**: Verify documentation quality
- [ ] Clear PostgreSQL setup instructions
- [ ] Working code examples tested
- [ ] Covers common use cases (semantic search, RAG)
- [ ] Distance semantics clearly explained
- [ ] Links to external resources (pgvector docs)

---

## Implementation Files Summary

### New Files (7 files)

1. **`src/fraiseql/sql/where/operators/vectors.py`**
   - Vector distance operator SQL builders
   - ~80 lines, 3 operator functions + helper

2. **`src/fraiseql/types/scalars/vector.py`** (optional)
   - Vector scalar type with minimal validation
   - ~40 lines if needed (may not be necessary)

3. **`tests/unit/sql/where/operators/test_vectors.py`**
   - Vector operator SQL generation tests
   - ~100 lines, 5+ test cases

4. **`tests/integration/graphql/schema/test_vector_filter.py`**
   - GraphQL schema generation tests
   - ~80 lines, 4+ test cases

5. **`tests/unit/types/test_vector_validation.py`**
   - Vector value validation tests
   - ~60 lines, 5+ test cases

6. **`tests/integration/test_vector_e2e.py`**
   - End-to-end integration tests
   - ~150 lines, 6+ test cases

7. **`docs/features/pgvector.md`** + **`docs/examples/semantic-search.md`**
   - Feature documentation and examples
   - ~400 lines combined

### Modified Files (4 files)

1. **`src/fraiseql/sql/where/core/field_detection.py`**
   - Add `VECTOR` to FieldType enum (1 line)
   - Add vector pattern detection (~20 lines)

2. **`src/fraiseql/sql/where/operators/__init__.py`**
   - Import vectors module (1 line)
   - Register 3 vector operators in OPERATOR_MAP (3 lines)

3. **`src/fraiseql/sql/graphql_where_generator.py`**
   - Add VectorFilter class (~25 lines)
   - Update _get_filter_type_for_field() (~15 lines)

4. **ORDER BY implementation files** (TBD in Phase 5)
   - Location to be determined via code exploration
   - Add vector distance operator support (~30 lines estimated)

### Lines of Code Estimate

- **New code**: ~900 lines (including tests and docs)
- **Modified code**: ~100 lines
- **Total impact**: ~1000 lines

---

## Testing Strategy

### Unit Tests (3 files, ~240 lines)
- Field type detection (VECTOR enum, pattern matching)
- SQL operator generation (3 distance operators)
- Value validation (type checking, no dimension checks)

### Integration Tests (2 files, ~230 lines)
- GraphQL schema generation (VectorFilter type)
- Type mapping (list[float] → VectorFilter for embeddings)
- E2E query flow (filter + orderBy + compose with other filters)

### PostgreSQL Setup for Tests
```sql
CREATE EXTENSION vector;

CREATE TABLE test_documents (
    id UUID PRIMARY KEY,
    title TEXT,
    embedding vector(384)
);

CREATE INDEX ON test_documents
USING hnsw (embedding vector_cosine_ops);
```

---

## Success Criteria

- [ ] All unit tests pass (field detection, SQL generation, validation)
- [ ] All integration tests pass (schema generation, type mapping)
- [ ] All E2E tests pass (real PostgreSQL + pgvector)
- [ ] GraphQL schema correctly generates VectorFilter
- [ ] SQL generates proper pgvector operators (`<=>`, `<->`, `<#>`)
- [ ] Works with PostgreSQL pgvector extension (v0.5.0+)
- [ ] No performance regression in existing queries
- [ ] Documentation complete and accurate
- [ ] Composable with existing filters (tenant isolation, timestamps, etc.)
- [ ] Follows FraiseQL architecture patterns (thin layer, zero magic)
- [ ] Distance semantics clearly documented (not similarities)

---

## FraiseQL Philosophy Alignment

| Principle | How This Implementation Adheres |
|-----------|--------------------------------|
| **Thin layer over PostgreSQL** | Exposes pgvector operators directly (`<=>`, `<->`, `<#>`) with no abstraction |
| **Zero magic** | Raw PostgreSQL distances returned, no conversion to similarities |
| **PostgreSQL-first** | Uses native pgvector types and operators, PostgreSQL handles validation |
| **Composable** | VectorFilter works with existing filters using established patterns |
| **Trust the database** | Dimension validation delegated to PostgreSQL, no metadata tracking |
| **Separation of concerns** | No index hints or warnings, users handle database optimization |
| **Naming transparency** | PostgreSQL terms (`cosine_distance`) not ML abstractions (`similarity`) |

---

## Benefits Delivered

✅ **Native PostgreSQL**: Pure pgvector, no abstractions or transformations
✅ **Type-safe**: GraphQL schema validation for vector operations
✅ **Composable**: Works with existing filters (tenant isolation, date ranges, full-text)
✅ **Performant**: HNSW indexes + FraiseQL's Rust pipeline (if applicable)
✅ **Simple**: 3 operators map directly to PostgreSQL (transparent behavior)
✅ **Zero infrastructure**: No vector DB needed, uses existing PostgreSQL
✅ **Predictable**: Raw distance values, no hidden conversions

---

## Use Cases Enabled

1. **Semantic Search**: Find similar documents/products by embedding similarity
2. **Recommendations**: "Products similar to this one" based on vector distance
3. **Duplicate Detection**: Find near-identical records using L2 distance
4. **RAG Systems**: Retrieve relevant context for LLMs via cosine distance
5. **Content Discovery**: Related articles, documents, images by embedding
6. **Hybrid Search**: Combine full-text search + vector similarity

---

## Estimated Effort

- **Phase 1**: 2-3 hours (field type infrastructure + tests)
- **Phase 2**: 3-4 hours (operator implementation + registration + tests)
- **Phase 3**: 2-3 hours (GraphQL schema + type mapping + tests)
- **Phase 4**: 1-2 hours (value validation + tests)
- **Phase 5**: 2-3 hours (ORDER BY support + tests)
- **Phase 6**: 3-4 hours (integration tests + documentation)

**Total**: 13-19 hours of development time

---

## References

- [PostgreSQL pgvector Extension](https://github.com/pgvector/pgvector) - Native vector similarity search
- [pgvector Operators](https://github.com/pgvector/pgvector#vector-operators) - Distance operator documentation
- [HNSW Index Performance](https://github.com/pgvector/pgvector#hnsw) - Index creation and tuning
- FraiseQL existing patterns: `network.py`, `ltree.py`, `fulltext.py` (specialized PostgreSQL types)

---

**Last Updated**: 2025-11-13
**Status**: Ready for Implementation
**Approval**: Design decisions applied based on FraiseQL philosophy
