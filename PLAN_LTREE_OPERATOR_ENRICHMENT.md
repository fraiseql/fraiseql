# LTREE Operator Enrichment - COMPLEX

**Complexity**: Complex | **Phased TDD Approach**

## Executive Summary
Enrich FraiseQL's existing LTREE support with ALL PostgreSQL ltree operators. Currently only 4 hierarchical operators are implemented (ancestor_of, descendant_of, matches_lquery, matches_ltxtquery). Add remaining 8+ operators for comprehensive hierarchical path querying with full test coverage.

## Architecture Context

**Current State Analysis**:
- File: `/home/lionel/code/fraiseql/src/fraiseql/sql/operator_strategies.py:773-905`
- Existing operators: `eq`, `neq`, `in`, `notin`, `ancestor_of`, `descendant_of`, `matches_lquery`, `matches_ltxtquery`
- Strategy: `LTreeOperatorStrategy` registered BEFORE generic strategies
- Tests: `/home/lionel/code/fraiseql/tests/unit/sql/where/test_ltree_operators_sql_building.py`

**PostgreSQL LTREE Operators Missing**:
1. **`first_ancestor`** - Get first N ancestors
2. **`first_descendant`** - Get first N descendants
3. **`nlevel`** - Get path level (number of labels)
4. **`index`** - Get position of sublabel
5. **`subpath`** - Extract subpath
6. **`lca`** - Lowest common ancestor
7. **`matches_ltree_array`** - Match any path in array (`?`)
8. **`concat`** - Concatenate paths (`||`)
9. **`text_to_ltree`** - Convert text to ltree

**Reference**: https://www.postgresql.org/docs/current/ltree.html

---

## PHASES

### Phase 1: Audit Current LTREE Implementation
**Objective**: Document current state and verify all existing operators work correctly

#### TDD Cycle:

**1. RED**: Write comprehensive tests for existing operators
- Test file: `tests/unit/sql/where/test_ltree_operators_complete.py` (new)
- Expected state: Some tests may expose edge cases

```python
class TestExistingLTreeOperators:
    """Verify all current LTREE operators work correctly."""

    def test_ltree_eq_operator(self):
        """Test exact path equality."""
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_eq_sql(path_sql, "top.science.physics")
        expected = "(data->>'path')::ltree = 'top.science.physics'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_ancestor_of_operator(self):
        """Test @> operator (path1 is ancestor of path2)."""
        # "top.science" @> "top.science.physics" = true
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_ancestor_of_sql(path_sql, "top.science.physics")
        expected = "(data->>'path')::ltree @> 'top.science.physics'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_descendant_of_operator(self):
        """Test <@ operator (path1 is descendant of path2)."""
        # "top.science.physics" <@ "top.science" = true
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_descendant_of_sql(path_sql, "top.science")
        expected = "(data->>'path')::ltree <@ 'top.science'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_matches_lquery(self):
        """Test ~ operator (path matches lquery pattern)."""
        # "top.science.physics" ~ "*.science.*" = true
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_matches_lquery_sql(path_sql, "*.science.*")
        expected = "(data->>'path')::ltree ~ '*.science.*'::lquery"
        assert result.as_string(None) == expected

    def test_ltree_matches_ltxtquery(self):
        """Test ? operator (path matches ltxtquery text search)."""
        # "top.science.physics" ? "science & physics" = true
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_matches_ltxtquery_sql(path_sql, "science & physics")
        expected = "(data->>'path')::ltree ? 'science & physics'::ltxtquery"
        assert result.as_string(None) == expected
```

**2. GREEN**: Fix any failing existing tests
- Verify all 8 current operators work
- Fix edge cases discovered

**3. REFACTOR**: Clean up existing implementation
- Ensure consistent SQL generation
- Verify proper ltree type casting
- Check error handling for invalid paths

**4. QA**: Baseline verification
- [ ] All 8 existing operators tested
- [ ] Edge cases handled (empty paths, invalid formats)
- [ ] Type casting consistent
- [ ] Error messages helpful

```bash
uv run pytest tests/unit/sql/where/test_ltree_operators_complete.py -v
```

---

### Phase 2: Add Level and Path Analysis Operators
**Objective**: Implement nlevel(), subpath(), index() operators

#### TDD Cycle:

**1. RED**: Write failing tests for path analysis operators
- Test file: `tests/unit/sql/where/test_ltree_path_analysis_operators.py`
- Expected failures: Operators not implemented

```python
class TestLTreePathAnalysisOperators:
    """Test LTREE path analysis operators."""

    def test_ltree_nlevel_operator(self):
        """Test nlevel(ltree) - returns number of labels in path."""
        # nlevel('top.science.physics') = 3
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_nlevel_sql(path_sql)
        expected = "nlevel((data->>'path')::ltree)"
        assert result.as_string(None) == expected

    def test_filter_by_path_depth(self):
        """Test filtering paths by depth using nlevel."""
        # Find all paths with exactly 3 levels
        # where={path: {nlevel_eq: 3}}
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_nlevel_eq_sql(path_sql, 3)
        expected = "nlevel((data->>'path')::ltree) = 3"
        assert result.as_string(None) == expected

    def test_ltree_subpath_operator(self):
        """Test subpath(ltree, offset, len) - extract subpath."""
        # subpath('top.science.physics.quantum', 1, 2) = 'science.physics'
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_subpath_sql(path_sql, offset=1, length=2)
        expected = "subpath((data->>'path')::ltree, 1, 2)"
        assert result.as_string(None) == expected

    def test_ltree_subpath_from_start(self):
        """Test subpath from start (offset=0)."""
        # subpath('top.science.physics', 0, 2) = 'top.science'
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_subpath_sql(path_sql, offset=0, length=2)
        expected = "subpath((data->>'path')::ltree, 0, 2)"
        assert result.as_string(None) == expected

    def test_ltree_index_operator(self):
        """Test index(a ltree, b ltree) - position of b in a, -1 if not found."""
        # index('top.science.physics', 'science') = 1
        path_sql = SQL("data->>'path'")
        sublabel = "science"
        result = ltree.build_ltree_index_sql(path_sql, sublabel)
        expected = "index((data->>'path')::ltree, 'science'::ltree)"
        assert result.as_string(None) == expected

    def test_ltree_index_not_found(self):
        """Test index returns -1 when sublabel not found."""
        # index('top.science.physics', 'technology') = -1
        # Can filter: where={path: {index_gte: 0}}  # Has sublabel
        pass
```

**2. GREEN**: Implement path analysis operators
- File to modify: `src/fraiseql/sql/operator_strategies.py` (LTreeOperatorStrategy)
- Add to operator list:

```python
class LTreeOperatorStrategy(BaseOperatorStrategy):
    def __init__(self) -> None:
        super().__init__([
            # ... existing operators ...
            # Path analysis operators:
            "nlevel_eq", "nlevel_gt", "nlevel_gte", "nlevel_lt", "nlevel_lte",
            "subpath",
            "index",
            "index_eq", "index_gte",  # For filtering by sublabel position
        ])

    def build_sql(self, path_sql, op, val, field_type=None):
        # ... existing code ...

        elif op.startswith("nlevel_"):
            # Extract comparison operator (eq, gt, gte, lt, lte)
            comparison = op.replace("nlevel_", "")
            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])
            nlevel_expr = Composed([SQL("nlevel("), casted_path, SQL(")")])

            comparison_ops = {"eq": "=", "gt": ">", "gte": ">=", "lt": "<", "lte": "<="}
            sql_op = comparison_ops[comparison]

            return Composed([nlevel_expr, SQL(f" {sql_op} "), Literal(val)])

        elif op == "subpath":
            # val is tuple (offset, length)
            offset, length = val
            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])
            return Composed([
                SQL("subpath("),
                casted_path,
                SQL(", "),
                Literal(offset),
                SQL(", "),
                Literal(length),
                SQL(")")
            ])

        elif op.startswith("index"):
            # index(path, sublabel) returns int position
            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])

            if op == "index":
                # Just return the index value
                return Composed([
                    SQL("index("),
                    casted_path,
                    SQL(", "),
                    Literal(val),
                    SQL("::ltree)")
                ])
            elif op == "index_eq":
                # Filter by exact position
                sublabel, position = val
                index_expr = Composed([
                    SQL("index("),
                    casted_path,
                    SQL(", "),
                    Literal(sublabel),
                    SQL("::ltree)")
                ])
                return Composed([index_expr, SQL(" = "), Literal(position)])
```

**3. REFACTOR**: Optimize operator implementation
- Consider adding helper methods for nlevel/index filtering
- Add input validation (offset >= 0, length > 0)
- Handle edge cases (empty paths, out of bounds)

**4. QA**: Verify path analysis operators
- [ ] nlevel operators work for depth filtering
- [ ] subpath extracts correct path segments
- [ ] index finds sublabel positions correctly
- [ ] Edge cases handled gracefully
- [ ] Integration tests pass

```bash
uv run pytest tests/unit/sql/where/test_ltree_path_analysis_operators.py -v
uv run pytest tests/integration/database/sql/test_ltree_filter_operations.py -v
```

---

### Phase 3: Add Path Manipulation Operators
**Objective**: Implement concat (||), lca (lowest common ancestor)

#### TDD Cycle:

**1. RED**: Write failing tests for path manipulation
- Test file: `tests/unit/sql/where/test_ltree_path_manipulation_operators.py`
- Expected failures: Operators not implemented

```python
class TestLTreePathManipulationOperators:
    """Test LTREE path manipulation operators."""

    def test_ltree_concat_operator(self):
        """Test || operator - concatenate paths."""
        # 'top.science' || 'physics.quantum' = 'top.science.physics.quantum'
        path_sql = SQL("data->>'path'")
        suffix = "physics.quantum"
        result = ltree.build_ltree_concat_sql(path_sql, suffix)
        expected = "(data->>'path')::ltree || 'physics.quantum'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_concat_single_label(self):
        """Test concatenating single label to path."""
        # 'top.science' || 'physics' = 'top.science.physics'
        path_sql = SQL("data->>'path'")
        result = ltree.build_ltree_concat_sql(path_sql, "physics")
        expected = "(data->>'path')::ltree || 'physics'::ltree"
        assert result.as_string(None) == expected

    def test_ltree_lca_operator(self):
        """Test lca(ltree[]) - lowest common ancestor."""
        # lca('top.science.physics', 'top.science.chemistry') = 'top.science'
        paths = ["top.science.physics", "top.science.chemistry", "top.science.biology"]
        result = ltree.build_ltree_lca_sql(paths)
        expected = "lca(ARRAY['top.science.physics'::ltree, 'top.science.chemistry'::ltree, 'top.science.biology'::ltree])"
        assert result.as_string(None) == expected

    def test_ltree_lca_two_paths(self):
        """Test lca with just two paths."""
        # lca('top.science', 'top.technology') = 'top'
        paths = ["top.science", "top.technology"]
        result = ltree.build_ltree_lca_sql(paths)
        expected = "lca(ARRAY['top.science'::ltree, 'top.technology'::ltree])"
        assert result.as_string(None) == expected

    def test_filter_by_common_ancestor(self):
        """Test filtering paths by their common ancestor with a target path."""
        # Find all paths that share 'top.science' as common ancestor
        # where={path: {lca_with: {paths: ['top.science.physics'], min_depth: 2}}}
        pass
```

**2. GREEN**: Implement path manipulation operators
- File to modify: `src/fraiseql/sql/operator_strategies.py` (LTreeOperatorStrategy)
- Add operators:

```python
class LTreeOperatorStrategy(BaseOperatorStrategy):
    def __init__(self) -> None:
        super().__init__([
            # ... existing operators ...
            # Path manipulation:
            "concat",  # Concatenate paths
            "lca",     # Lowest common ancestor
        ])

    def build_sql(self, path_sql, op, val, field_type=None):
        # ... existing code ...

        elif op == "concat":
            # path1 || path2 - concatenate two ltree paths
            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])
            return Composed([
                casted_path,
                SQL(" || "),
                Literal(val),
                SQL("::ltree")
            ])

        elif op == "lca":
            # lca(ARRAY[path1, path2, ...]) - lowest common ancestor
            if not isinstance(val, list):
                raise TypeError(f"lca operator requires a list of paths, got {type(val)}")

            # Build ARRAY['path1'::ltree, 'path2'::ltree, ...]
            parts = [SQL("lca(ARRAY[")]
            for i, path in enumerate(val):
                if i > 0:
                    parts.append(SQL(", "))
                parts.extend([Literal(path), SQL("::ltree")])
            parts.append(SQL("])")

            return Composed(parts)
```

**3. REFACTOR**: Enhance path manipulation
- Consider adding `concat_eq` for filtering by concatenated path
- Add `lca_eq` to find paths with specific common ancestor
- Validate path formats before concatenation
- Handle empty path arrays in lca

**4. QA**: Verify path manipulation
- [ ] concat operator joins paths correctly
- [ ] lca finds correct common ancestor
- [ ] Works with various path depths
- [ ] Integration tests validate database behavior

```bash
uv run pytest tests/unit/sql/where/test_ltree_path_manipulation_operators.py -v
```

---

### Phase 4: Add Array Matching Operators
**Objective**: Implement array-based matching (? with ltree[], @>, <@)

#### TDD Cycle:

**1. RED**: Write failing tests for array matching
- Test file: `tests/unit/sql/where/test_ltree_array_operators.py`
- Expected failures: Array operators not implemented

```python
class TestLTreeArrayOperators:
    """Test LTREE array matching operators."""

    def test_ltree_matches_any_in_array(self):
        """Test ? operator with ltree array - matches any path."""
        # 'top.science.physics' ? ARRAY['top.science.*', 'top.tech.*'] = true
        path_sql = SQL("data->>'path'")
        patterns = ["top.science.*", "top.technology.*"]
        result = ltree.build_ltree_matches_any_sql(path_sql, patterns)
        expected = "(data->>'path')::ltree ? ARRAY['top.science.*', 'top.technology.*']"
        # Note: This uses lquery array, not ltree array
        assert result.as_string(None).startswith("(data->>'path')::ltree ?")

    def test_ltree_array_contains_path(self):
        """Test @> operator with path array - array contains path."""
        # ARRAY['top.science', 'top.technology'] @> 'top.science' = true
        paths_array = ["top.science", "top.technology", "top.arts"]
        target_path = "top.science"
        result = ltree.build_ltree_array_contains_sql(paths_array, target_path)
        expected = "ARRAY['top.science'::ltree, 'top.technology'::ltree, 'top.arts'::ltree] @> ARRAY['top.science'::ltree]"
        assert result.as_string(None) == expected

    def test_ltree_path_in_array(self):
        """Test <@ operator - path is contained in array."""
        # 'top.science' <@ ARRAY['top.science', 'top.technology'] = true
        path_sql = SQL("data->>'path'")
        valid_paths = ["top.science", "top.technology", "top.arts"]
        result = ltree.build_ltree_in_array_sql(path_sql, valid_paths)
        expected = "(data->>'path')::ltree <@ ARRAY['top.science'::ltree, 'top.technology'::ltree, 'top.arts'::ltree]"
        assert result.as_string(None) == expected
```

**2. GREEN**: Implement array matching operators
- File to modify: `src/fraiseql/sql/operator_strategies.py` (LTreeOperatorStrategy)
- Add operators:

```python
class LTreeOperatorStrategy(BaseOperatorStrategy):
    def __init__(self) -> None:
        super().__init__([
            # ... existing operators ...
            # Array matching:
            "matches_any_lquery",  # ? with lquery array
            "array_contains",      # ltree[] @> ltree
            "in_array",            # ltree <@ ltree[]
        ])

    def build_sql(self, path_sql, op, val, field_type=None):
        # ... existing code ...

        elif op == "matches_any_lquery":
            # path ? ARRAY[lquery1, lquery2, ...]
            if not isinstance(val, list):
                raise TypeError(f"matches_any_lquery requires a list, got {type(val)}")

            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])

            # Build ARRAY[lquery1, lquery2, ...]
            parts = [casted_path, SQL(" ? ARRAY[")]
            for i, pattern in enumerate(val):
                if i > 0:
                    parts.append(SQL(", "))
                parts.extend([Literal(pattern)])  # PostgreSQL will cast to lquery
            parts.append(SQL("]"))

            return Composed(parts)

        elif op == "in_array":
            # path <@ ARRAY[path1, path2, ...]
            if not isinstance(val, list):
                raise TypeError(f"in_array requires a list, got {type(val)}")

            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])

            parts = [casted_path, SQL(" <@ ARRAY[")]
            for i, path in enumerate(val):
                if i > 0:
                    parts.append(SQL(", "))
                parts.extend([Literal(path), SQL("::ltree")])
            parts.append(SQL("]")]

            return Composed(parts)
```

**3. REFACTOR**: Optimize array operations
- Consider performance implications of large arrays
- Add validation for array sizes
- Handle empty arrays gracefully

**4. QA**: Verify array matching
- [ ] matches_any_lquery works with pattern arrays
- [ ] in_array checks path membership correctly
- [ ] Performance acceptable with reasonable array sizes
- [ ] Edge cases (empty arrays, single element) handled

```bash
uv run pytest tests/unit/sql/where/test_ltree_array_operators.py -v
```

---

### Phase 5: Integration Testing - All Operators
**Objective**: End-to-end validation of all 20+ LTREE operators

#### TDD Cycle:

**1. RED**: Write comprehensive integration tests
- Test file: `tests/integration/database/sql/test_ltree_all_operators_integration.py`
- Expected failures: Some operators may not work end-to-end

```python
@pytest.fixture
async def ltree_test_data(db):
    """Create test data with hierarchical paths."""
    class Category:
        id: int
        name: str
        path: LTree

    categories = [
        {"name": "Top", "path": "top"},
        {"name": "Science", "path": "top.science"},
        {"name": "Physics", "path": "top.science.physics"},
        {"name": "Quantum", "path": "top.science.physics.quantum"},
        {"name": "Chemistry", "path": "top.science.chemistry"},
        {"name": "Technology", "path": "top.technology"},
        {"name": "Computing", "path": "top.technology.computing"},
        {"name": "AI", "path": "top.technology.computing.ai"},
        {"name": "Arts", "path": "top.arts"},
        {"name": "Music", "path": "top.arts.music"},
    ]

    for cat in categories:
        await db.Category.create(cat)

    return db.Category

class TestAllLTreeOperators:
    """Integration tests for all LTREE operators."""

    async def test_basic_operators(self, ltree_test_data):
        """Test eq, neq, in, notin."""
        # Exact match
        result = await ltree_test_data.find(where={"path": {"eq": "top.science"}})
        assert len(result) == 1
        assert result[0]["name"] == "Science"

        # In list
        result = await ltree_test_data.find(where={
            "path": {"in": ["top.science", "top.technology", "top.arts"]}
        })
        assert len(result) == 3

    async def test_hierarchical_operators(self, ltree_test_data):
        """Test ancestor_of, descendant_of."""
        # Find all descendants of 'top.science'
        result = await ltree_test_data.find(where={
            "path": {"descendant_of": "top.science"}
        })
        assert len(result) == 3  # physics, quantum, chemistry

        # Find all ancestors of 'top.science.physics.quantum'
        result = await ltree_test_data.find(where={
            "path": {"ancestor_of": "top.science.physics.quantum"}
        })
        assert len(result) == 3  # top, science, physics

    async def test_pattern_matching_operators(self, ltree_test_data):
        """Test matches_lquery, matches_ltxtquery."""
        # Find all paths with 'science' anywhere
        result = await ltree_test_data.find(where={
            "path": {"matches_lquery": "*.science.*"}
        })
        assert len(result) >= 2  # physics, chemistry, quantum

    async def test_path_analysis_operators(self, ltree_test_data):
        """Test nlevel, subpath, index."""
        # Find all paths with exactly 3 levels
        result = await ltree_test_data.find(where={
            "path": {"nlevel_eq": 3}
        })
        assert len(result) == 4  # science.physics, science.chemistry, technology.computing, arts.music

        # Find paths with depth >= 4
        result = await ltree_test_data.find(where={
            "path": {"nlevel_gte": 4}
        })
        assert len(result) == 2  # quantum, ai

    async def test_path_manipulation_operators(self, ltree_test_data):
        """Test concat, lca."""
        # Test concatenation (may need custom query)
        # Test lowest common ancestor calculation
        pass

    async def test_array_operators(self, ltree_test_data):
        """Test matches_any_lquery, in_array."""
        # Find paths matching any pattern in array
        result = await ltree_test_data.find(where={
            "path": {"matches_any_lquery": ["top.science.*", "top.arts.*"]}
        })
        assert len(result) >= 4  # science branches + arts branches
```

**2. GREEN**: Fix integration issues
- Debug any operator failures
- Fix SQL generation issues
- Handle database-level errors

**3. REFACTOR**: Optimize query performance
- Add appropriate database indexes
- Consider GiST indexes for ltree columns:
  ```sql
  CREATE INDEX path_gist_idx ON categories USING GIST (path);
  ```
- Optimize array operations

**4. QA**: Full integration validation
- [ ] All 20+ operators work end-to-end
- [ ] Performance acceptable with realistic data volumes
- [ ] Database indexes improve query performance
- [ ] Error handling works correctly

```bash
uv run pytest tests/integration/database/sql/test_ltree_all_operators_integration.py -v
uv run pytest tests/integration/database/sql/test_end_to_end_ltree_filtering.py -v
```

---

### Phase 6: Documentation and Examples
**Objective**: Comprehensive LTREE operator documentation

#### TDD Cycle:

**1. RED**: Write documentation examples that must work
- Test file: `tests/integration/examples/test_ltree_complete_example.py`
- Expected failures: Example code doesn't execute

**2. GREEN**: Create working examples
- File to create: `examples/ltree_hierarchical_data_example.py`
- Comprehensive example:

```python
"""
LTREE Hierarchical Data Example - Complete Operator Demonstration
"""
from fraiseql import Database
from fraiseql.types import LTree

class Category:
    id: int
    name: str
    path: LTree
    description: str

db = Database(...)

# ============================================
# BASIC OPERATORS
# ============================================

# Exact match
physics = await db.Category.find_one(where={
    "path": {"eq": "top.science.physics"}
})

# Multiple exact matches
main_categories = await db.Category.find(where={
    "path": {"in": ["top.science", "top.technology", "top.arts"]}
})

# ============================================
# HIERARCHICAL OPERATORS
# ============================================

# Find all descendants of 'top.science'
science_subtopics = await db.Category.find(where={
    "path": {"descendant_of": "top.science"}
})
# Returns: physics, chemistry, biology, quantum, etc.

# Find all ancestors of 'top.science.physics.quantum'
quantum_path = await db.Category.find(where={
    "path": {"ancestor_of": "top.science.physics.quantum"}
})
# Returns: top, top.science, top.science.physics

# ============================================
# PATTERN MATCHING
# ============================================

# Match paths with 'science' anywhere
science_related = await db.Category.find(where={
    "path": {"matches_lquery": "*.science.*"}
})

# Match paths with 'science' AND 'physics'
physics_topics = await db.Category.find(where={
    "path": {"matches_ltxtquery": "science & physics"}
})

# Match multiple patterns
stem_topics = await db.Category.find(where={
    "path": {"matches_any_lquery": ["*.science.*", "*.technology.*", "*.engineering.*"]}
})

# ============================================
# PATH ANALYSIS
# ============================================

# Find all top-level categories (depth = 2: top.X)
top_level = await db.Category.find(where={
    "path": {"nlevel_eq": 2}
})

# Find deep categories (depth >= 4)
deep_categories = await db.Category.find(where={
    "path": {"nlevel_gte": 4}
})

# Find categories at specific depth range
mid_level = await db.Category.find(where={
    "path": {"nlevel_gte": 3, "nlevel_lte": 4}
})

# ============================================
# ADVANCED USAGE
# ============================================

# Build dynamic category tree
async def get_category_tree(root_path: str):
    """Get all descendants organized by depth."""
    categories = await db.Category.find(where={
        "path": {"descendant_of": root_path}
    })

    # Group by depth
    tree = {}
    for cat in categories:
        depth = cat["path"].count(".")
        if depth not in tree:
            tree[depth] = []
        tree[depth].append(cat)

    return tree

# Find sibling categories (same parent, same depth)
async def get_siblings(category_path: str):
    """Find all categories at same level with same parent."""
    # Extract parent path
    parent = ".".join(category_path.split(".")[:-1])
    depth = category_path.count(".")

    siblings = await db.Category.find(where={
        "path": {
            "descendant_of": parent,
            "nlevel_eq": depth + 1,
            "neq": category_path  # Exclude self
        }
    })
    return siblings
```

**3. REFACTOR**: Add comprehensive documentation
- File to update: `docs/core/types-and-schema.md` (LTREE section)
- Create: `docs/advanced/ltree-hierarchical-queries.md`
- Update: `docs/reference/operators.md` with all LTREE operators

Documentation sections:
1. **LTREE Basics** - Path format, validation, storage
2. **Operator Reference** - All 20+ operators with examples
3. **Performance Tuning** - GiST indexes, query optimization
4. **Common Patterns** - Category trees, org charts, file systems
5. **Migration Guide** - Converting from adjacency list to ltree

**4. QA**: Documentation quality
- [ ] All examples executable and correct
- [ ] All operators documented
- [ ] Performance guidance included
- [ ] Migration guide tested
- [ ] Common patterns demonstrated

```bash
uv run pytest tests/integration/examples/test_ltree_complete_example.py -v
```

---

## Success Criteria

### Operator Coverage (Complete PostgreSQL LTREE Support)

**Basic Operators** (4 existing):
- [x] `eq`, `neq`, `in`, `notin`

**Hierarchical Operators** (4 existing):
- [x] `ancestor_of` (@>)
- [x] `descendant_of` (<@)
- [x] `matches_lquery` (~)
- [x] `matches_ltxtquery` (?)

**Path Analysis Operators** (NEW - 8 operators):
- [ ] `nlevel_eq`, `nlevel_gt`, `nlevel_gte`, `nlevel_lt`, `nlevel_lte`
- [ ] `subpath`
- [ ] `index`, `index_eq`, `index_gte`

**Path Manipulation Operators** (NEW - 2 operators):
- [ ] `concat` (||)
- [ ] `lca` (lowest common ancestor)

**Array Matching Operators** (NEW - 3 operators):
- [ ] `matches_any_lquery` (? with lquery[])
- [ ] `in_array` (<@ with ltree[])
- [ ] `array_contains` (@> with ltree[])

### Testing Requirements
- [ ] Unit tests for all 21+ operators
- [ ] Integration tests with real database
- [ ] Performance benchmarks
- [ ] Edge case coverage (empty paths, deep nesting, special characters)

### Documentation Requirements
- [ ] Operator reference guide
- [ ] Complete examples
- [ ] Performance tuning guide
- [ ] Migration guide from other hierarchical patterns

### Quality Requirements
- [ ] All tests pass
- [ ] Code coverage > 95% for LTREE code
- [ ] Performance acceptable (< 100ms for typical queries)
- [ ] Follows FraiseQL operator strategy pattern

---

## Implementation Notes

### PostgreSQL LTREE Operator Reference

| Operator | PostgreSQL | FraiseQL Operator | Description |
|----------|------------|-------------------|-------------|
| `=` | `ltree = ltree` | `eq` | Equal |
| `<>` | `ltree <> ltree` | `neq` | Not equal |
| `@>` | `ltree @> ltree` | `ancestor_of` | Left is ancestor of right |
| `<@` | `ltree <@ ltree` | `descendant_of` | Left is descendant of right |
| `~` | `ltree ~ lquery` | `matches_lquery` | Match lquery pattern |
| `?` | `ltree ? ltxtquery` | `matches_ltxtquery` | Match ltxtquery |
| `?` | `ltree ? lquery[]` | `matches_any_lquery` | Match any pattern |
| `<@` | `ltree <@ ltree[]` | `in_array` | In array |
| `@>` | `ltree[] @> ltree` | `array_contains` | Array contains |
| `\|\|` | `ltree \|\| ltree` | `concat` | Concatenate paths |
| `nlevel(ltree)` | Function | `nlevel_*` | Number of labels |
| `subpath(ltree,int,int)` | Function | `subpath` | Extract subpath |
| `index(ltree,ltree)` | Function | `index*` | Position of sublabel |
| `lca(ltree[])` | Function | `lca` | Lowest common ancestor |

### LTREE Path Format Rules
- Labels separated by dots: `top.science.physics`
- Labels can contain: `A-Za-z0-9_`
- Maximum label length: 256 characters
- Maximum path length: 65535 bytes
- Case-sensitive by default

### Performance Optimization
```sql
-- Essential index for LTREE queries
CREATE INDEX categories_path_gist_idx ON categories USING GIST (path);

-- For exact equality searches
CREATE INDEX categories_path_btree_idx ON categories USING BTREE (path);
```

### Common Use Cases
1. **Category Hierarchies** - Product categories, taxonomies
2. **Organizational Charts** - Company structure, reporting lines
3. **File Systems** - Directory trees, document organization
4. **Geographic Hierarchies** - Country > State > City > Neighborhood
5. **Classification Systems** - Dewey Decimal, taxonomies

---

**Estimated Effort**: 12-16 hours
- Phase 1: 2 hours (audit existing)
- Phase 2: 3-4 hours (path analysis operators)
- Phase 3: 2-3 hours (path manipulation)
- Phase 4: 2-3 hours (array operators)
- Phase 5: 2-3 hours (integration tests)
- Phase 6: 2-3 hours (documentation)

**Dependencies**:
- PostgreSQL with ltree extension: `CREATE EXTENSION IF NOT EXISTS ltree;`
- Existing LTreeOperatorStrategy (working baseline)

**Risk Areas**:
- LTREE extension availability in target databases
- Performance with very deep hierarchies (> 10 levels)
- Special character handling in path labels
- Array operator performance with large arrays
