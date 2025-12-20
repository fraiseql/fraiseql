# FraiseQL GraphQL Spec Compliance Gap Analysis - QA REVIEW

**Date:** December 17, 2025
**Reviewed By:** Architecture QA
**Previous Analysis:** graphql-spec-compliance-gap-analysis-2025-12-17.md
**Status:** ✅ FINAL ASSESSMENT READY FOR IMPLEMENTATION

---

## Executive Summary

The original gap analysis correctly identified 5 easy-to-implement gaps, but **prioritized them incorrectly for FraiseQL's actual architecture**.

FraiseQL is **not a traditional GraphQL server**—it's a **view-centric query engine** where:
- All joins are **pre-computed in `tv_*` materialized views**
- Business logic validation happens in **SQL functions**, not directives
- Data fetching is **inherently optimized** by the database schema design
- Fragment support is the **primary ergonomic need**

**Corrected Priority (8-11 hours total, not 28):**

| # | Gap | Effort | FraiseQL Fit | Decision |
|---|-----|--------|--------------|----------|
| 1 | Nested Fragments | 2-3h | ✅ **Perfect** | **IMPLEMENT 1st** |
| 2 | Fragment Cycle Detection | 3-4h | ✅ **Critical** | **IMPLEMENT 2nd** |
| 3 | View/Metadata Directives | 2-4h | ✅ **Perfect fit** | **IMPLEMENT 3rd** |
| 4 | Auto DataLoader | 4-6h | ⚠️ **Skip** | SKIP (By Design) |
| 5 | HTTP Streaming | 6-8h | ❌ **Skip** | SKIP (Out of Scope) |

---

## Part 1: What Was Right About the Original Analysis

✅ **Nested Fragments** - Correctly identified as high value
✅ **Fragment Cycle Detection** - Correctly identified as stability improvement
✅ **Effort estimates** - Realistic and achievable
✅ **Implementation plans** - Detailed and executable
✅ **Test strategy** - Comprehensive coverage

---

## Part 2: What Was Wrong (Architectural Misunderstanding)

### ❌ **Gap #3: Auto-integrated DataLoaders (4-6 hrs)**

**Original Assessment:** "IMPLEMENT THIS SECOND - Eliminates N+1 queries"

**Corrected Assessment:** ⚠️ **SKIP - Unnecessary by design**

#### Why It's Wrong:

FraiseQL's view-centric architecture **eliminates N+1 queries at the schema level**:

```python
# Traditional GraphQL problem (N+1 queries):
query {
  users {
    id
    name
    posts {              # ← 1 query for users + N queries for posts
      id
      title
    }
  }
}

# FraiseQL solution:
# Define tv_user_with_posts view (pre-joined at DB level)
@fraiseql.type(sql_source="tv_user_with_posts")
class UserWithPosts:
    id: UUID
    name: str
    posts: list[dict]    # ← Already in denormalized view, 1 query total
```

**Why DataLoaders are unnecessary:**
1. ✅ All required relationships are **pre-computed in `tv_*` views**
2. ✅ PostgreSQL JSONB arrays/objects handle **1-to-many relationships**
3. ✅ No batching needed because **data is already denormalized**
4. ✅ IVM (Incremental View Maintenance) handles **view refresh**, not DataLoaders

**When would you need DataLoaders?**
- ❌ Only if querying normalized `tb_*` base tables instead of views
- ❌ Only if your schema forces N+1 queries (architectural problem)
- ❌ Not with FraiseQL's intended usage pattern

**Verdict:** Skip this gap entirely. It solves a problem FraiseQL's architecture intentionally eliminated.

---

### ❌ **Gap #2: Custom Business Logic Directives (2-4 hrs)**

**Original Assessment:** "IMPLEMENT THIS THIRD - Enterprise value"

**Corrected Assessment:** ⚠️ **REFRAME - Implement as metadata directives only**

#### Why the Original Was Wrong:

FraiseQL validates **at the database layer**, not in GraphQL directives:

```python
# ❌ WRONG PATTERN (goes against FraiseQL)
@rate_limit(calls: Int!)              # GraphQL directive
@access_level(minLevel: Int!)         # GraphQL directive
@validate(pattern: String!)           # GraphQL directive

# ✅ CORRECT PATTERN (FraiseQL way)
# Rate limiting: use database rate_limit table + fn_check_rate_limit()
# Access level: computed in tv_user view with role checks
# Validation: PostgreSQL CHECK constraint or stored procedure

# SQL Function Layer (Write-Side):
CREATE FUNCTION fn_create_user(...)
  SECURITY DEFINER
  SET search_path = core, public
AS $$
BEGIN
  -- Validation happens here
  IF email NOT VALID THEN RAISE EXCEPTION ...; END IF;
  -- Access control happens here
  IF NOT has_permission(...) THEN RAISE EXCEPTION ...; END IF;
  -- ...
END $$;

# View Layer (Read-Side):
CREATE MATERIALIZED VIEW tv_user AS
SELECT id, name, email,
       CASE WHEN role IN ('admin', 'superadmin') THEN true ELSE false END as is_admin
FROM tb_user
WHERE tenant_id = current_setting('app.current_tenant_id')::uuid;
```

#### The Right Implementation:

**Use directives for schema metadata, not business logic:**

```python
# ✅ CORRECT: View/Metadata Directives
@view_cached(ttl: Int!)              # Control materialized view refresh TTL
@depends_on(views: [String!]!)       # Document upstream view dependencies
@requires_function(name: String!)    # Declare required SQL function
@cost_units(estimate: Float!)        # Semantic cost for query planning

# Usage Example:
@fraiseql.type(sql_source="tv_user_with_extended_profile")
class UserWithProfile:
    id: UUID
    name: str
    profile: dict = Field(
        description="Extended profile data",
        directives=[
            ViewCachedDirective(ttl=3600),           # Refresh hourly
            DependsOnDirective(views=["tb_user", "tb_profile"]),
            RequiresFunctionDirective(name="fn_validate_user_profile")
        ]
    )
```

**These directives:**
- 📋 Document schema intentions
- 🔍 Enable tooling (view dependency graphs)
- 📊 Support cost analysis
- 🔒 Enforce requirements (function must exist)
- **BUT** do not execute business logic

**Verdict:** Implement as **metadata directives only**, not business logic directives.

---

### ❌ **Gap #4: HTTP Streaming / @stream @defer (6-8 hrs)**

**Original Assessment:** "IMPLEMENT THIS FOURTH - Advanced capability"

**Corrected Assessment:** ❌ **SKIP - Out of scope for FraiseQL**

#### Why:

1. **FraiseQL queries are bounded**: Views return pre-shaped, complete results
2. **No streaming benefit**: Data is already optimized at schema level
3. **Protocol overhead**: SSE/streaming adds complexity without performance gain
4. **Architecture mismatch**: Incremental delivery doesn't align with view-based results
5. **WebSocket already works**: For actual streaming needs (subscriptions)

**Verdict:** Skip entirely. Focus on bounded queries.

---

## Part 3: Correct Implementation Roadmap

### Phase 1: Query Ergonomics (Week 1) - 5-7 hours

#### Gap #1: Nested Field Fragments (2-3 hours)

**What:** Expand fragment spreads recursively in nested field selections

**Why it matters:**
- ✅ Complex denormalized views have many fields
- ✅ Fragment reuse becomes critical as schemas grow
- ✅ Enables composition of view selectors

**Current code location:** `src/fraiseql/core/fragment_resolver.py:40-62`

**Implementation:**
```python
# Current: resolve() at root level only
def resolve(sel: SelectionNode) -> None:
    if sel.kind == "field":
        field_node = cast("FieldNode", sel)
        result.append(field_node)
        # ❌ MISSING: Recursively process field_node.selection_set

# Fixed: Recursive fragment resolution
def resolve(sel: SelectionNode) -> None:
    if sel.kind == "field":
        field_node = cast("FieldNode", sel)
        # ✅ NEW: If field has selections, resolve them recursively
        if field_node.selection_set:
            nested_fields = resolve_all_fields(
                field_node.selection_set,
                fragments,
                typename=None  # Type from schema
            )
            # Attach resolved nested fields
        result.append(field_node)
```

**Tests (5+ cases):**
- Fragment spread in nested field selection
- Multiple levels of nesting
- Mix of inline fragments and spreads
- Fragment with alias in nested context
- Deeply nested fragment references

**Risk:** Low - extending existing pattern
**Dependencies:** None
**Success criterion:** All 5 test cases pass, no regressions

---

#### Gap #5: Fragment Cycle Detection (3-4 hours)

**What:** Detect and reject circular fragment references

**Why it matters:**
- ✅ Prevents DoS via infinite fragment recursion
- ✅ Catches configuration errors early
- ✅ Enables safe fragment validation at parse time

**Current code location:** `src/fraiseql/core/fragment_resolver.py:46-50`

**Implementation:**

```python
# File: src/fraiseql/core/fragment_resolver.py

def resolve_all_fields(
    selection_set: SelectionSetNode,
    fragments: dict[str, FragmentDefinitionNode],
    typename: str | None = None,
    visited_fragments: set[str] | None = None,  # ✅ NEW
) -> list[FieldNode]:
    """Resolve fields with cycle detection"""
    if visited_fragments is None:
        visited_fragments = set()

    result: list[FieldNode] = []

    def resolve(sel: SelectionNode) -> None:
        if sel.kind == "fragment_spread":
            frag_spread = cast("FragmentSpreadNode", sel)
            name = frag_spread.name.value

            # ✅ NEW: Cycle detection
            if name in visited_fragments:
                raise ValueError(f"Circular fragment reference detected: {name}")

            if name not in fragments:
                raise ValueError(f"Fragment '{name}' not found")

            # ✅ NEW: Track visited fragments
            new_visited = visited_fragments | {name}
            frag = fragments[name]

            # ✅ NEW: Pass visited set to nested resolve calls
            for frag_sel in frag.selection_set.selections:
                resolve_with_visited(frag_sel, new_visited)
```

**Tests (10+ cases):**
- Direct self-reference: `fragment A → A`
- Mutual cycle: `A → B → A`
- Chain cycle: `A → B → C → A`
- Valid non-cycles: `A → B → C` (no back-edge)
- Complex mixed cycles with multiple paths

**Risk:** Low - defensive programming
**Dependencies:** fragment_resolver.py
**Success criterion:** All cycle patterns detected, valid fragments still work

---

### Phase 2: Schema Semantics (Week 2) - 2-4 hours

#### Gap #2: View/Metadata Directives (2-4 hours)

**What:** Directives that document schema intentions and enable tooling

**Why it matters:**
- ✅ Documents view dependencies for maintenance
- ✅ Enables automatic view refresh strategy generation
- ✅ Supports query cost analysis
- ✅ Enforces schema requirements

**Implementation:**

```python
# File: src/fraiseql/gql/schema_directives.py (NEW)

from abc import ABC, abstractmethod
from typing import Any

class SchemaDirective(ABC):
    """Base class for schema metadata directives"""

    @abstractmethod
    def validate(self, context: dict[str, Any]) -> None:
        """Validate directive requirements"""

class ViewCachedDirective(SchemaDirective):
    """@view_cached(ttl: Int!) - Materialized view refresh TTL"""

    def __init__(self, ttl: int):
        self.ttl = ttl

    def validate(self, context: dict[str, Any]) -> None:
        """Ensure TTL is positive"""
        if self.ttl <= 0:
            raise ValueError(f"TTL must be positive, got {self.ttl}")

class DependsOnDirective(SchemaDirective):
    """@depends_on(views: [String!]!) - Upstream view/table dependencies"""

    def __init__(self, views: list[str]):
        self.views = views

    def validate(self, context: dict[str, Any]) -> None:
        """Ensure all dependencies exist"""
        schema = context.get("schema")
        for view in self.views:
            if not schema.has_table_or_view(view):
                raise ValueError(f"View/table '{view}' not found in schema")

class RequiresFunctionDirective(SchemaDirective):
    """@requires_function(name: String!) - SQL function must exist"""

    def __init__(self, name: str):
        self.name = name

    def validate(self, context: dict[str, Any]) -> None:
        """Ensure function exists in database"""
        db = context.get("db")
        if not db.function_exists(self.name):
            raise ValueError(f"Required function '{self.name}' not found")

class CostUnitsDirective(SchemaDirective):
    """@cost_units(estimate: Float!) - Query complexity/cost estimate"""

    def __init__(self, estimate: float):
        self.estimate = estimate

    def validate(self, context: dict[str, Any]) -> None:
        if self.estimate < 0:
            raise ValueError(f"Cost estimate must be non-negative")
```

**Schema Definition:**
```python
# File: src/fraiseql/gql/schema_builder.py

# Add directive definitions to GraphQL schema
schema_directives = [
    GraphQLDirective(
        name="view_cached",
        locations=[DirectiveLocation.FIELD_DEFINITION],
        args={
            "ttl": GraphQLArgument(GraphQLNonNull(GraphQLInt)),
        },
        description="Control materialized view cache/refresh TTL in seconds",
    ),
    GraphQLDirective(
        name="depends_on",
        locations=[DirectiveLocation.FIELD_DEFINITION],
        args={
            "views": GraphQLArgument(GraphQLNonNull(GraphQLList(GraphQLNonNull(GraphQLString)))),
        },
        description="Document upstream view dependencies",
    ),
    GraphQLDirective(
        name="requires_function",
        locations=[DirectiveLocation.FIELD_DEFINITION],
        args={
            "name": GraphQLArgument(GraphQLNonNull(GraphQLString)),
        },
        description="SQL function must exist in database",
    ),
    GraphQLDirective(
        name="cost_units",
        locations=[DirectiveLocation.FIELD_DEFINITION],
        args={
            "estimate": GraphQLArgument(GraphQLNonNull(GraphQLFloat)),
        },
        description="Query complexity/cost estimate for planning",
    ),
]
```

**Usage Example:**
```python
@fraiseql.type(sql_source="tv_user_with_profile")
class UserWithProfile:
    id: UUID = Field(directives=["@view_cached(ttl: 3600)"])

    name: str

    profile: dict = Field(
        directives=[
            "@depends_on(views: [\"tb_user\", \"tb_profile\"])",
            "@requires_function(name: \"fn_validate_profile\")",
            "@cost_units(estimate: 2.5)",
            "@view_cached(ttl: 7200)"
        ]
    )
```

**Tests (8+ cases):**
- TTL validation (positive, negative, zero)
- View dependency validation (existing, missing views)
- Function requirement validation (existing, missing functions)
- Cost unit validation (zero, positive, large values)
- Multiple directives on same field

**Risk:** Low - metadata only, no execution
**Dependencies:** GraphQL schema builder
**Success criterion:** All directives validate correctly, schema introspection shows directives

---

## Part 4: Rejected Gaps (Explicit Non-Implementation)

### ❌ Gap #3: Auto-integrated DataLoaders

**Rejection Reason:** Architectural mismatch with view-centric design

**Why:**
- Denormalized views eliminate N+1 queries at schema level
- DataLoaders add complexity without benefit
- Graph doesn't require batching when data is pre-joined

**If someone asks "Why no DataLoaders?":**
> "FraiseQL uses denormalized materialized views that pre-compute all required relationships. This eliminates N+1 queries at the database schema level, making DataLoaders unnecessary. When you query a denormalized view, you get all related data in a single database roundtrip."

---

### ❌ Gap #4: HTTP Streaming / @stream @defer

**Rejection Reason:** Out of scope for bounded query results

**Why:**
- FraiseQL queries return complete, pre-shaped results from views
- Incremental delivery doesn't align with view-based architecture
- WebSocket subscriptions already handle streaming needs
- Protocol overhead not justified by use cases

**If someone asks "Why no @stream/@defer?":**
> "FraiseQL's view-based architecture returns bounded, pre-shaped results that are already optimized at the database layer. Streaming doesn't add value for these use cases. For actual streaming needs, use WebSocket subscriptions."

---

## Part 5: Success Criteria

### Phase 1: Query Ergonomics (Weeks 1-2)

#### Nested Fragments Success:
- [ ] All 5+ nested fragment test cases pass
- [ ] No regressions in existing fragment tests (full test suite passes)
- [ ] Complex denormalized views work with nested spreads
- [ ] Performance < 5% variance

#### Fragment Cycle Detection Success:
- [ ] All 10+ cycle detection test cases pass
- [ ] Direct self-references rejected
- [ ] Mutual cycles rejected
- [ ] Chain cycles rejected
- [ ] Valid fragments still work
- [ ] Error messages clear and actionable

### Phase 2: Schema Semantics (Weeks 3-4)

#### View/Metadata Directives Success:
- [ ] All 8+ directive validation tests pass
- [ ] Schema introspection shows all directives
- [ ] TTL validation working
- [ ] View dependency validation working
- [ ] Function requirement validation working
- [ ] Documentation complete
- [ ] Examples show usage patterns

---

## Part 6: Risk Analysis

### Risk 1: Breaking Fragment Resolution
**Probability:** Low
**Mitigation:** Run full test suite after each change, use feature branches

### Risk 2: Performance Regression
**Probability:** Low
**Mitigation:** Benchmark fragment resolution time, profile memory usage

### Risk 3: Directive Validation Too Strict
**Probability:** Medium
**Mitigation:** Make directives optional, provide migration guide for existing schemas

### Risk 4: Fragment Cycles Hard to Debug
**Probability:** Low
**Mitigation:** Clear error messages with cycle path visualization

---

## Part 7: Implementation Order & Dependencies

```
Phase 1: Query Ergonomics
├── Gap #1: Nested Fragments (2-3h)
│   └── Builds on: fragment_resolver.py
│   └── No blocking dependencies
│
└── Gap #5: Fragment Cycle Detection (3-4h)
    └── Builds on: Nested Fragments (optional, but good to do first)
    └── Depends on: fragment_resolver.py

Phase 2: Schema Semantics
└── Gap #2: View/Metadata Directives (2-4h)
    └── Builds on: schema_builder.py
    └── No blocking dependencies
```

**Can be done in parallel:**
- Gap #1 and Gap #5 (different modules, can have separate PRs)
- Gap #2 (independent of fragments)

---

## Part 8: What NOT to Do

### 🚫 Don't implement "business logic directives"
- Validation goes in SQL CHECK constraints, not directives
- Rate limiting goes in rate_limit tables + stored procedures
- Access control goes in `tv_*` views with role filters

### 🚫 Don't implement DataLoaders
- They solve a non-problem in FraiseQL's architecture
- Denormalization eliminates N+1 queries
- Adds unnecessary complexity

### 🚫 Don't implement HTTP Streaming
- FraiseQL queries return bounded results
- WebSocket subscriptions handle actual streaming
- Protocol overhead not justified

---

## Conclusion

**FraiseQL's GraphQL spec compliance roadmap should focus on:**

1. **Query Ergonomics** (5-7h)
   - Nested fragments for complex view queries
   - Cycle detection for schema safety

2. **Schema Semantics** (2-4h)
   - Metadata directives for documentation and tooling
   - Cost analysis support for query planning

**Total effort: 8-11 hours** (vs. original 28 hours for all gaps)

**Impact:**
- ✅ Improves from ~90% to ~93% spec compliance
- ✅ Maintains FraiseQL's architectural integrity
- ✅ Focuses on real developer pain points
- ✅ Aligns with view-centric design philosophy

**This roadmap prioritizes pragmatism over spec completeness—exactly what FraiseQL stands for.**

---

**Status:** ✅ Ready for implementation planning
**Next Steps:** Create detailed implementation tickets for Phase 1 & 2
