# Phase R2: Implement Missing Operators [GREEN]

**Status**: BLOCKED (waiting for R1)
**Priority**: 🟢 HIGH
**Duration**: 2 days
**Risk**: MEDIUM

---

## Objective

Implement all missing operators to bring FraiseQL to feature parity with current implementation. Support vector distance, fulltext search, array operations, and additional string operators.

---

## Context

**Current Operators** (in `where_clause.py`):
- ✅ Comparison: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`
- ✅ Containment: `in`, `nin`
- ✅ String: `contains`, `icontains`, `startswith`, `istartswith`, `endswith`, `iendswith`
- ✅ Null: `isnull`

**Missing Operators** (causing 14+ test failures):
- ❌ String: `ilike`, `like` (explicit)
- ❌ Vector: `cosine_distance`, `l2_distance`, `l1_distance`, `hamming_distance`, `jaccard_distance`
- ❌ Fulltext: `matches`, `plain_query`, `phrase_query`, `websearch_query`, `rank_gt`, `rank_lt`, `rank_cd_gt`, `rank_cd_lt`
- ❌ Array: `array_eq`, `array_neq`, `array_contains`, `array_contained_by`, `array_overlaps`, `array_length_eq`, `array_length_gt`, `array_length_lt`, `array_length_gte`, `array_any_eq`, `array_all_eq`

**Impact**: 14 test files failing due to unsupported operators

---

## Implementation Steps

### Step 1: Add String Operators (1 hour)

**Location**: `src/fraiseql/where_clause.py`

**Add to operator constants**:
```python
STRING_OPERATORS = {
    "contains": "LIKE",
    "icontains": "ILIKE",
    "startswith": "LIKE",
    "istartswith": "ILIKE",
    "endswith": "LIKE",
    "iendswith": "ILIKE",
    "like": "LIKE",      # NEW: explicit LIKE
    "ilike": "ILIKE",    # NEW: explicit ILIKE
}
```

**Update `_build_like_pattern()`**:
```python
def _build_like_pattern(self) -> str:
    """Build LIKE pattern from operator and value."""
    if self.operator in ("contains", "icontains"):
        return f"%{self.value}%"
    elif self.operator in ("startswith", "istartswith"):
        return f"{self.value}%"
    elif self.operator in ("endswith", "iendswith"):
        return f"%{self.value}"
    elif self.operator in ("like", "ilike"):
        # Explicit LIKE/ILIKE - user provides pattern as-is
        return str(self.value)
    else:
        return str(self.value)
```

**Test**:
```python
# Add to tests/unit/test_where_clause.py
def test_explicit_like_operator_to_sql(self):
    """Test explicit LIKE operator preserves user pattern."""
    condition = FieldCondition(
        field_path=["name"],
        operator="like",
        value="Test%",  # User-provided pattern
        lookup_strategy="sql_column",
        target_column="name",
    )

    sql, params = condition.to_sql()
    sql_str = sql.as_string(None)
    assert "LIKE" in sql_str
    assert params[0] == "Test%"  # Pattern preserved

def test_ilike_operator_to_sql(self):
    """Test ILIKE operator works."""
    condition = FieldCondition(
        field_path=["name"],
        operator="ilike",
        value="%test%",
        lookup_strategy="sql_column",
        target_column="name",
    )

    sql, params = condition.to_sql()
    sql_str = sql.as_string(None)
    assert "ILIKE" in sql_str
    assert params[0] == "%test%"
```

**Verification**:
```bash
uv run pytest tests/unit/test_where_clause.py -k "like" -v
uv run pytest tests/integration/database/repository/test_dynamic_filter_construction.py -v
```

---

### Step 2: Add Vector Distance Operators (3 hours)

**Location**: `src/fraiseql/where_clause.py`

**Add operator constants**:
```python
VECTOR_OPERATORS = {
    "cosine_distance": "<=>",
    "l2_distance": "<->",
    "l1_distance": "<+>",
    "hamming_distance": "<~>",
    "jaccard_distance": "<%>",
}

ALL_OPERATORS = {
    **COMPARISON_OPERATORS,
    **CONTAINMENT_OPERATORS,
    **STRING_OPERATORS,
    **NULL_OPERATORS,
    **VECTOR_OPERATORS,  # NEW
}
```

**Add to `FieldCondition.to_sql()`**:
```python
def to_sql(self) -> tuple[Composed, list[Any]]:
    """Generate SQL for this condition."""
    params = []

    # ... existing FK and JSONB logic ...

    elif self.lookup_strategy == "sql_column":
        # Direct SQL column: status = %s
        sql_op = ALL_OPERATORS[self.operator]

        # NEW: Handle vector operators
        if self.operator in VECTOR_OPERATORS:
            # Vector distance: embedding <=> %s < threshold
            # Operator is already comparison, value should be threshold
            vector_op = VECTOR_OPERATORS[self.operator]
            sql = Composed([
                Identifier(self.target_column),
                SQL(f" {vector_op} "),
                SQL("%s")
            ])
            params.append(self.value)  # Vector for comparison

        # Existing containment, null, string logic...
        elif self.operator in CONTAINMENT_OPERATORS:
            # ... existing code ...
```

**IMPORTANT**: Vector operators need special handling
- Value is the target vector (as list or array)
- Operator returns distance (float)
- Usually combined with comparison: `embedding <=> [0.1, 0.2, ...] < 0.5`

**Enhanced Implementation**:
```python
# Vector operators actually return distance, so we need to support:
# {"embedding": {"cosine_distance": {"vector": [...], "lt": 0.5}}}

# This requires nested operator handling. For now, simplify:
# {"embedding": {"cosine_distance_lt": [..., 0.5]}}

# OR use separate operator for threshold:
# {"embedding": {"cosine_distance": [0.1, 0.2, ...]}, "cosine_threshold": {"lt": 0.5}}

# RECOMMENDED: Use comparison-specific vector operators
VECTOR_DISTANCE_OPERATORS = {
    "cosine_distance_lt": "(<=> %s) < %s",
    "cosine_distance_lte": "(<=> %s) <= %s",
    "l2_distance_lt": "(<-> %s) < %s",
    "l2_distance_lte": "(<-> %s) <= %s",
    # etc.
}
```

**Simplified Implementation** (Phase R2):
For now, implement basic vector distance:
```python
VECTOR_OPERATORS = {
    "cosine_distance": "<=>",
    "l2_distance": "<->",
    "l1_distance": "<+>",
    "hamming_distance": "<~>",
    "jaccard_distance": "<%>",
}

# In to_sql():
if self.operator in VECTOR_OPERATORS:
    # Vector distance comparison
    # Value format: {"vector": [...], "threshold": 0.5, "comparison": "lt"}
    # OR simplified: value is (vector, threshold) tuple
    vector_op = VECTOR_OPERATORS[self.operator]

    if isinstance(self.value, dict):
        vector = self.value.get("vector")
        threshold = self.value.get("threshold")
        comparison = self.value.get("comparison", "lt")  # Default <
    elif isinstance(self.value, (list, tuple)) and len(self.value) == 2:
        vector, threshold = self.value
        comparison = "lt"
    else:
        raise ValueError(f"Vector operator requires dict or (vector, threshold) tuple")

    comp_op = "<" if comparison == "lt" else "<=" if comparison == "lte" else ">"

    sql = Composed([
        SQL("("),
        Identifier(self.target_column),
        SQL(f" {vector_op} "),
        SQL("%s"),
        SQL(f") {comp_op} "),
        SQL("%s")
    ])
    params.extend([vector, threshold])
```

**Test**:
```python
def test_cosine_distance_operator(self):
    """Test vector cosine distance operator."""
    condition = FieldCondition(
        field_path=["embedding"],
        operator="cosine_distance",
        value={"vector": [0.1, 0.2, 0.3], "threshold": 0.5},
        lookup_strategy="sql_column",
        target_column="embedding",
    )

    sql, params = condition.to_sql()
    sql_str = sql.as_string(None)

    assert "<=>" in sql_str
    assert "<" in sql_str
    assert len(params) == 2
    assert params[0] == [0.1, 0.2, 0.3]
    assert params[1] == 0.5
```

**Verification**:
```bash
uv run pytest tests/integration/test_vector_e2e.py -v
```

---

### Step 3: Add Fulltext Search Operators (3 hours)

**Location**: `src/fraiseql/where_clause.py`

**Add operator constants**:
```python
FULLTEXT_OPERATORS = {
    "matches": "@@",
    "plain_query": "@@",
    "phrase_query": "@@",
    "websearch_query": "@@",
    "rank_gt": ">",
    "rank_lt": "<",
    "rank_cd_gt": ">",
    "rank_cd_lt": "<",
}
```

**Implementation**:
```python
# In to_sql():
elif self.operator in FULLTEXT_OPERATORS:
    # Fulltext search operators
    if self.operator == "matches":
        # Basic fulltext: column @@ to_tsquery(%s)
        sql = Composed([
            Identifier(self.target_column),
            SQL(" @@ to_tsquery("),
            SQL("%s"),
            SQL(")")
        ])
        params.append(self.value)

    elif self.operator == "plain_query":
        # Plain query: column @@ plainto_tsquery(%s)
        sql = Composed([
            Identifier(self.target_column),
            SQL(" @@ plainto_tsquery("),
            SQL("%s"),
            SQL(")")
        ])
        params.append(self.value)

    elif self.operator == "phrase_query":
        # Phrase query: column @@ phraseto_tsquery(%s)
        sql = Composed([
            Identifier(self.target_column),
            SQL(" @@ phraseto_tsquery("),
            SQL("%s"),
            SQL(")")
        ])
        params.append(self.value)

    elif self.operator == "websearch_query":
        # Websearch query: column @@ websearch_to_tsquery(%s)
        sql = Composed([
            Identifier(self.target_column),
            SQL(" @@ websearch_to_tsquery("),
            SQL("%s"),
            SQL(")")
        ])
        params.append(self.value)

    elif self.operator in ("rank_gt", "rank_lt"):
        # Rank comparison: ts_rank(column, to_tsquery(%s)) > %s
        # Value: {"query": "search", "threshold": 0.5}
        if isinstance(self.value, dict):
            query = self.value.get("query")
            threshold = self.value.get("threshold")
        else:
            raise ValueError("rank_* operators require dict with query and threshold")

        comp = ">" if self.operator == "rank_gt" else "<"
        sql = Composed([
            SQL("ts_rank("),
            Identifier(self.target_column),
            SQL(", to_tsquery("),
            SQL("%s"),
            SQL(f")) {comp} "),
            SQL("%s")
        ])
        params.extend([query, threshold])

    elif self.operator in ("rank_cd_gt", "rank_cd_lt"):
        # Cover density rank: ts_rank_cd(...)
        if isinstance(self.value, dict):
            query = self.value.get("query")
            threshold = self.value.get("threshold")
        else:
            raise ValueError("rank_cd_* operators require dict with query and threshold")

        comp = ">" if self.operator == "rank_cd_gt" else "<"
        sql = Composed([
            SQL("ts_rank_cd("),
            Identifier(self.target_column),
            SQL(", to_tsquery("),
            SQL("%s"),
            SQL(f")) {comp} "),
            SQL("%s")
        ])
        params.extend([query, threshold])
```

**Test**:
```python
def test_fulltext_matches_operator(self):
    """Test fulltext @@ operator."""
    condition = FieldCondition(
        field_path=["search_vector"],
        operator="matches",
        value="search & term",
        lookup_strategy="sql_column",
        target_column="search_vector",
    )

    sql, params = condition.to_sql()
    sql_str = sql.as_string(None)

    assert "@@" in sql_str
    assert "to_tsquery" in sql_str
    assert params[0] == "search & term"
```

**Verification**:
```bash
uv run pytest tests/integration/database/repository/test_fulltext_filter.py -v
```

---

### Step 4: Add Array Operators (3 hours)

**Location**: `src/fraiseql/where_clause.py`

**Add operator constants**:
```python
ARRAY_OPERATORS = {
    "array_eq": "=",
    "array_neq": "!=",
    "array_contains": "@>",
    "array_contained_by": "<@",
    "array_overlaps": "&&",
    "array_length_eq": "=",
    "array_length_gt": ">",
    "array_length_lt": "<",
    "array_length_gte": ">=",
    "array_any_eq": "= ANY",
    "array_all_eq": "= ALL",
}
```

**Implementation**:
```python
# In to_sql():
elif self.operator in ARRAY_OPERATORS:
    # Array operators
    if self.operator in ("array_eq", "array_neq"):
        # Array equality: column = ARRAY[...]
        op = "=" if self.operator == "array_eq" else "!="
        sql = Composed([
            Identifier(self.target_column),
            SQL(f" {op} "),
            SQL("%s")
        ])
        params.append(self.value)

    elif self.operator in ("array_contains", "array_contained_by", "array_overlaps"):
        # Array containment: column @> ARRAY[...]
        op = ARRAY_OPERATORS[self.operator]
        sql = Composed([
            Identifier(self.target_column),
            SQL(f" {op} "),
            SQL("%s")
        ])
        params.append(self.value)

    elif self.operator in ("array_length_eq", "array_length_gt", "array_length_lt", "array_length_gte"):
        # Array length: array_length(column, 1) > %s
        op = ARRAY_OPERATORS[self.operator]
        sql = Composed([
            SQL("array_length("),
            Identifier(self.target_column),
            SQL(", 1) "),
            SQL(f"{op} "),
            SQL("%s")
        ])
        params.append(self.value)

    elif self.operator in ("array_any_eq", "array_all_eq"):
        # ANY/ALL: %s = ANY(column)
        op = "ANY" if self.operator == "array_any_eq" else "ALL"
        sql = Composed([
            SQL("%s = "),
            SQL(f"{op}("),
            Identifier(self.target_column),
            SQL(")")
        ])
        params.append(self.value)
```

**Test**:
```python
def test_array_contains_operator(self):
    """Test array @> operator."""
    condition = FieldCondition(
        field_path=["tags"],
        operator="array_contains",
        value=["tag1", "tag2"],
        lookup_strategy="sql_column",
        target_column="tags",
    )

    sql, params = condition.to_sql()
    sql_str = sql.as_string(None)

    assert "@>" in sql_str
    assert params[0] == ["tag1", "tag2"]

def test_array_length_gt_operator(self):
    """Test array_length() > n."""
    condition = FieldCondition(
        field_path=["tags"],
        operator="array_length_gt",
        value=5,
        lookup_strategy="sql_column",
        target_column="tags",
    )

    sql, params = condition.to_sql()
    sql_str = sql.as_string(None)

    assert "array_length" in sql_str
    assert ">" in sql_str
    assert params[0] == 5
```

**Verification**:
```bash
uv run pytest tests/integration/database/repository/test_array_filter.py -v
```

---

### Step 5: Update Operator Registry Documentation (1 hour)

**Create**: `docs/where-operators.md`

**Content**:
```markdown
# WHERE Clause Operators Reference

Complete reference for all supported operators in FraiseQL WHERE clauses.

## Comparison Operators

| Operator | SQL | Description | Example |
|----------|-----|-------------|---------|
| `eq` | `=` | Equal | `{"age": {"eq": 25}}` |
| `neq` | `!=` | Not equal | `{"status": {"neq": "inactive"}}` |
| `gt` | `>` | Greater than | `{"score": {"gt": 90}}` |
| `gte` | `>=` | Greater or equal | `{"age": {"gte": 18}}` |
| `lt` | `<` | Less than | `{"price": {"lt": 100}}` |
| `lte` | `<=` | Less or equal | `{"age": {"lte": 65}}` |

## Containment Operators

| Operator | SQL | Description | Example |
|----------|-----|-------------|---------|
| `in` | `IN` | Value in list | `{"status": {"in": ["active", "pending"]}}` |
| `nin` | `NOT IN` | Value not in list | `{"status": {"nin": ["deleted", "archived"]}}` |

## String Operators

| Operator | SQL | Description | Example |
|----------|-----|-------------|---------|
| `contains` | `LIKE` | Contains substring (case-sensitive) | `{"name": {"contains": "John"}}` |
| `icontains` | `ILIKE` | Contains substring (case-insensitive) | `{"name": {"icontains": "john"}}` |
| `startswith` | `LIKE` | Starts with (case-sensitive) | `{"email": {"startswith": "admin"}}` |
| `istartswith` | `ILIKE` | Starts with (case-insensitive) | `{"email": {"istartswith": "admin"}}` |
| `endswith` | `LIKE` | Ends with (case-sensitive) | `{"email": {"endswith": "@example.com"}}` |
| `iendswith` | `ILIKE` | Ends with (case-insensitive) | `{"email": {"iendswith": "@EXAMPLE.COM"}}` |
| `like` | `LIKE` | Custom LIKE pattern | `{"name": {"like": "J%n"}}` |
| `ilike` | `ILIKE` | Custom ILIKE pattern | `{"name": {"ilike": "j%n"}}` |

## Null Operators

| Operator | SQL | Description | Example |
|----------|-----|-------------|---------|
| `isnull` | `IS NULL` / `IS NOT NULL` | Check null | `{"deleted_at": {"isnull": True}}` |

## Vector Distance Operators (PostgreSQL pgvector)

| Operator | SQL | Description | Example |
|----------|-----|-------------|---------|
| `cosine_distance` | `<=>` | Cosine distance | `{"embedding": {"cosine_distance": {"vector": [...], "threshold": 0.5}}}` |
| `l2_distance` | `<->` | Euclidean (L2) distance | `{"embedding": {"l2_distance": {"vector": [...], "threshold": 1.0}}}` |
| `l1_distance` | `<+>` | Manhattan (L1) distance | `{"embedding": {"l1_distance": {"vector": [...], "threshold": 2.0}}}` |
| `hamming_distance` | `<~>` | Hamming distance (binary) | `{"bits": {"hamming_distance": {"vector": [...], "threshold": 10}}}` |
| `jaccard_distance` | `<%>` | Jaccard distance | `{"bits": {"jaccard_distance": {"vector": [...], "threshold": 0.3}}}` |

## Fulltext Search Operators (PostgreSQL tsvector)

| Operator | SQL | Description | Example |
|----------|-----|-------------|---------|
| `matches` | `@@` | Fulltext match | `{"search_vector": {"matches": "python & django"}}` |
| `plain_query` | `@@` | Plain text query | `{"search_vector": {"plain_query": "python django"}}` |
| `phrase_query` | `@@` | Phrase query | `{"search_vector": {"phrase_query": "machine learning"}}` |
| `websearch_query` | `@@` | Websearch-style query | `{"search_vector": {"websearch_query": "python OR ruby"}}` |
| `rank_gt` | `>` | Rank greater than | `{"search_vector": {"rank_gt": {"query": "python", "threshold": 0.5}}}` |
| `rank_lt` | `<` | Rank less than | `{"search_vector": {"rank_lt": {"query": "python", "threshold": 0.1}}}` |
| `rank_cd_gt` | `>` | Cover density rank > | `{"search_vector": {"rank_cd_gt": {"query": "python", "threshold": 0.5}}}` |
| `rank_cd_lt` | `<` | Cover density rank < | `{"search_vector": {"rank_cd_lt": {"query": "python", "threshold": 0.1}}}` |

## Array Operators (PostgreSQL arrays)

| Operator | SQL | Description | Example |
|----------|-----|-------------|---------|
| `array_eq` | `=` | Array equals | `{"tags": {"array_eq": ["python", "django"]}}` |
| `array_neq` | `!=` | Array not equals | `{"tags": {"array_neq": ["java", "spring"]}}` |
| `array_contains` | `@>` | Contains values | `{"tags": {"array_contains": ["python"]}}` |
| `array_contained_by` | `<@` | Contained by | `{"tags": {"array_contained_by": ["python", "django", "flask"]}}` |
| `array_overlaps` | `&&` | Has common elements | `{"tags": {"array_overlaps": ["python", "ruby"]}}` |
| `array_length_eq` | `=` | Array length equals | `{"tags": {"array_length_eq": 3}}` |
| `array_length_gt` | `>` | Array length > | `{"tags": {"array_length_gt": 5}}` |
| `array_length_lt` | `<` | Array length < | `{"tags": {"array_length_lt": 10}}` |
| `array_length_gte` | `>=` | Array length >= | `{"tags": {"array_length_gte": 1}}` |
| `array_any_eq` | `= ANY` | Value equals any element | `{"tags": {"array_any_eq": "python"}}` |
| `array_all_eq` | `= ALL` | Value equals all elements | `{"tags": {"array_all_eq": "python"}}` |

## Usage Examples

### Basic Filtering
\`\`\`python
await repo.find("users", where={"age": {"gte": 18}})
\`\`\`

### Multiple Conditions (AND)
\`\`\`python
await repo.find("users", where={
    "age": {"gte": 18},
    "status": {"eq": "active"}
})
\`\`\`

### OR Conditions
\`\`\`python
await repo.find("users", where={
    "OR": [
        {"status": {"eq": "active"}},
        {"status": {"eq": "pending"}}
    ]
})
\`\`\`

### Nested Filters (FK)
\`\`\`python
await repo.find("allocations", where={
    "machine": {"id": {"eq": machine_id}}
})
\`\`\`

### Nested Filters (JSONB)
\`\`\`python
await repo.find("allocations", where={
    "device": {"name": {"icontains": "printer"}}
})
\`\`\`

### Vector Search
\`\`\`python
embedding = [0.1, 0.2, 0.3, ...]
await repo.find("documents", where={
    "embedding": {
        "cosine_distance": {
            "vector": embedding,
            "threshold": 0.5
        }
    }
})
\`\`\`

### Fulltext Search
\`\`\`python
await repo.find("posts", where={
    "search_vector": {"websearch_query": "python machine learning"}
})
\`\`\`

### Array Filters
\`\`\`python
await repo.find("posts", where={
    "tags": {"array_contains": ["python", "tutorial"]}
})
\`\`\`
```

---

## Verification Commands

### After Each Step
```bash
# Step 1: String operators
uv run pytest tests/unit/test_where_clause.py -k "like" -v
uv run pytest tests/integration/database/repository/test_dynamic_filter_construction.py -v

# Step 2: Vector operators
uv run pytest tests/unit/test_where_clause.py -k "vector or distance" -v
uv run pytest tests/integration/test_vector_e2e.py -v

# Step 3: Fulltext operators
uv run pytest tests/unit/test_where_clause.py -k "fulltext or rank" -v
uv run pytest tests/integration/database/repository/test_fulltext_filter.py -v

# Step 4: Array operators
uv run pytest tests/unit/test_where_clause.py -k "array" -v
uv run pytest tests/integration/database/repository/test_array_filter.py -v
```

### Full Verification
```bash
# All operator tests
uv run pytest tests/ -v -k "operator or filter" --tb=short

# Full suite
uv run pytest tests/ -v

# Target: 100% pass rate
```

---

## Acceptance Criteria

### Operators Implemented ✅
- [ ] String: `like`, `ilike`
- [ ] Vector: All 5 distance operators
- [ ] Fulltext: All 8 operators
- [ ] Array: All 11 operators

### Tests Passing ✅
- [ ] `tests/integration/test_vector_e2e.py`: 9/9 passing
- [ ] `tests/integration/database/repository/test_fulltext_filter.py`: 8/8 passing
- [ ] `tests/integration/database/repository/test_array_filter.py`: 11/11 passing
- [ ] `tests/integration/database/repository/test_dynamic_filter_construction.py`: All passing

### Documentation ✅
- [ ] `docs/where-operators.md` created
- [ ] All operators documented with examples
- [ ] Usage examples comprehensive

### Quality ✅
- [ ] All operators have unit tests
- [ ] SQL generation correct for all operators
- [ ] Parameter binding safe for all operators
- [ ] No code duplication

---

## DO NOT

❌ **DO NOT** implement operators without tests
❌ **DO NOT** use string interpolation (SQL injection risk)
❌ **DO NOT** skip validation for complex value formats
❌ **DO NOT** move to Phase R3 until all tests passing

---

## Rollback Plan

**If operator implementation too complex**:
- Implement subset of operators (basic string + common operators)
- Mark advanced operators as "not yet supported" with clear error
- Document limitations in CHANGELOG
- Plan for future phase

---

## Time Estimates

| Step | Optimistic | Realistic | Pessimistic |
|------|-----------|-----------|-------------|
| 1. String operators | 0.5h | 1h | 2h |
| 2. Vector operators | 2h | 3h | 5h |
| 3. Fulltext operators | 2h | 3h | 5h |
| 4. Array operators | 2h | 3h | 5h |
| 5. Documentation | 0.5h | 1h | 2h |
| **TOTAL** | **7h** | **11h** | **19h** |

**Realistic Timeline**: 2 days (8h/day = 16h includes testing)

---

## Progress Tracking

### Day 1
- [ ] Steps 1-2 complete
- [ ] String + Vector operators working

### Day 2
- [ ] Steps 3-5 complete
- [ ] All operators implemented
- [ ] All tests passing

---

**Phase Status**: BLOCKED (waiting for R1)
**Previous Phase**: [phase-r1-fix-critical-blockers.md](phase-r1-fix-critical-blockers.md)
**Next Phase**: [phase-r3-whereinput-integration.md](phase-r3-whereinput-integration.md)
