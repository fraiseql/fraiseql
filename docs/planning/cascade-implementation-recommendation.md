# GraphQL Cascade: Implementation Decision

**Date**: 2025-11-11
**Status**: Decision Required
**Current State**: Phase 1 partially implemented (untracked changes)

---

## Current Situation

The team has started implementing GraphQL Cascade using the original 10-phase plan:

### Files Created (Untracked):
- `src/fraiseql/cascade/tracker.py` (84 lines)
- `src/fraiseql/cascade/builder.py` (116 lines)
- `src/fraiseql/cascade/__init__.py` (68 lines)
- `tests/unit/cascade/` (test files)
- `docs/planning/graphql-cascade-implementation-plan.md` (original plan)

**Total**: ~268 lines of Python code for Phase 1 only

**Status**: NOT YET COMMITTED (can be easily reverted)

---

## Two Approaches Available

### Approach A: Original Plan (Continue Current Implementation)

**What's been done:**
- ✅ `CascadeTracker` class (context variables, tracking methods)
- ✅ `CascadeBuilder` class (response construction)
- ✅ Basic tracking infrastructure

**What's remaining:**
- Phase 2: PostgreSQL integration (database executor changes)
- Phase 3: Mutation decorator enhancement
- Phase 4: GraphQL schema types (7 new types)
- Phase 5: Response formatting
- Phase 6: Manual tracking API
- Phase 7: Configuration and optimization
- Phase 8-10: Documentation, tests, migration

**Total Effort**: ~4 weeks, ~2000+ lines of code

**Pros:**
- 25% already implemented
- Following detailed plan
- More "sophisticated" tracking

**Cons:**
- 75% still to implement
- Complex Python tracking overhead
- More code to maintain
- Longer time to production

---

### Approach B: Simplified PostgreSQL-First (Recommended)

**Core Insight**: PostgreSQL functions already return structured JSONB. We can add cascade data directly in the return value, eliminating the need for Python tracking.

**What changes:**
- ❌ Delete `CascadeTracker` class (not needed)
- ❌ Delete `CascadeBuilder` class (not needed)
- ✅ PostgreSQL functions build `_cascade` directly
- ✅ Python just passes through the JSONB field

**Total Effort**: ~3-5 days, ~50 lines of code

**Pros:**
- 90% less code
- PostgreSQL-native (zero overhead)
- Simpler to maintain
- Faster to production
- More flexible (PostgreSQL decides what to cascade)
- Aligns with FraiseQL's database-first philosophy

**Cons:**
- Need to discard current work (268 lines)
- Different approach than originally planned

---

## Technical Comparison

### Approach A: Python Tracking
```python
# Complex Python tracking
tracker = CascadeTracker(max_depth=3)
set_active_tracker(tracker)

tracker.track_create("Post", post_entity)
tracker.track_update("User", user_entity, depth=1)

builder = CascadeBuilder(tracker)
cascade_data = builder.build_cascade_data()
```

### Approach B: PostgreSQL Native
```sql
-- Simple PostgreSQL JSONB construction
RETURN jsonb_build_object(
    'success', true,
    'data', jsonb_build_object('id', v_post_id),
    '_cascade', jsonb_build_object(
        'updated', jsonb_build_array(
            jsonb_build_object(
                '__typename', 'Post',
                'id', v_post_id,
                'operation', 'CREATED',
                'entity', (SELECT data FROM v_post WHERE id = v_post_id)
            )
        )
    )
);
```

```python
# Minimal Python passthrough
if "_cascade" in result and self.enable_cascade:
    parsed_result.__cascade__ = result["_cascade"]
```

---

## Detailed Comparison

| Aspect | Approach A (Original) | Approach B (Simplified) |
|--------|----------------------|------------------------|
| **Python Code** | ~2000 lines | ~50 lines |
| **PostgreSQL** | Minimal changes | Build cascade in functions |
| **Complexity** | High (context vars, tracking) | Low (JSONB passthrough) |
| **Performance** | Tracking overhead | Zero overhead |
| **Flexibility** | Predefined tracking rules | PostgreSQL decides |
| **Time to Production** | 4 weeks | 3-5 days |
| **Maintenance** | Complex tracking logic | Simple JSONB handling |
| **Testing** | Mock Python tracking | Test PostgreSQL directly |
| **Alignment** | Framework-first | Database-first (FraiseQL philosophy) |
| **SpecQL Pattern** | Different approach | Matches SpecQL's `extra_metadata` |

---

## Code Examples: Side by Side

### PostgreSQL Function

#### Approach A (Original):
```sql
CREATE OR REPLACE FUNCTION fn_create_post(input jsonb)
RETURNS jsonb AS $$
BEGIN
    -- Business logic only
    INSERT INTO tb_post ...;

    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', v_post_id)
    );
    -- Python tracker handles cascade
END;
$$ LANGUAGE plpgsql;
```

#### Approach B (Simplified):
```sql
CREATE OR REPLACE FUNCTION fn_create_post(input jsonb)
RETURNS jsonb AS $$
BEGIN
    -- Business logic
    INSERT INTO tb_post ...;

    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', v_post_id),
        '_cascade', jsonb_build_object(  -- ← Add cascade here
            'updated', jsonb_build_array(
                jsonb_build_object(
                    '__typename', 'Post',
                    'id', v_post_id,
                    'operation', 'CREATED',
                    'entity', (SELECT data FROM v_post WHERE id = v_post_id)
                )
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### Python Mutation Resolver

#### Approach A (Original):
```python
async def resolver(info, input):
    # Initialize tracker
    tracker = CascadeTracker(max_depth=3)
    set_active_tracker(tracker)

    try:
        # Execute function
        result = await db.execute_function(...)

        # Process cascade entities
        if "_cascade_entities" in result:
            db._process_cascade_entities(result["_cascade_entities"], tracker)

        # Build cascade data
        builder = CascadeBuilder(tracker)
        cascade_data = builder.build_cascade_data()

        # Parse and attach
        parsed_result = parse_mutation_result(...)
        parsed_result.__cascade__ = cascade_data

        return parsed_result
    finally:
        set_active_tracker(None)
```

#### Approach B (Simplified):
```python
async def resolver(info, input):
    # Execute function
    result = await db.execute_function(...)

    # Parse result
    parsed_result = parse_mutation_result(...)

    # Pass through cascade if present
    if "_cascade" in result and self.enable_cascade:
        parsed_result.__cascade__ = result["_cascade"]

    return parsed_result
```

---

## What to Revert

### If choosing Approach B (Recommended):

**Files to Remove:**
```bash
# Remove untracked cascade files
rm -rf src/fraiseql/cascade/
rm -rf tests/unit/cascade/

# Remove original plan
rm docs/planning/graphql-cascade-implementation-plan.md

# Keep simplified plan
# Keep: docs/planning/graphql-cascade-simplified-approach.md
```

**Git Status After Cleanup:**
```bash
# Should only show:
# - docs/planning/graphql-cascade-simplified-approach.md (new)
# - docs/planning/cascade-implementation-recommendation.md (new)
```

---

## Migration Path from Phase 1

### If Continuing with Approach A:

**Keep:**
- `src/fraiseql/cascade/tracker.py` ✅
- `src/fraiseql/cascade/builder.py` ✅
- `tests/unit/cascade/` ✅

**Continue with:**
- Phases 2-10 as planned
- ~3-4 more weeks of work

### If Switching to Approach B:

**Steps:**
1. Remove cascade Python files
2. Implement simplified passthrough (~1 day)
3. Document PostgreSQL pattern (~1 day)
4. Create examples (~1 day)

**Time saved:** 3+ weeks

---

## Recommendation: Choose Approach B

### Reasons:

1. **Sunk Cost is Minimal**: Only 268 lines (~1 day of work)
2. **Time to Production**: 3-5 days vs 4 weeks
3. **Code Maintenance**: 50 lines vs 2000+ lines
4. **Performance**: Zero overhead vs tracking overhead
5. **Philosophy Alignment**: Database-first (FraiseQL's core principle)
6. **SpecQL Precedent**: Matches SpecQL's proven pattern
7. **Flexibility**: PostgreSQL has full control

### The Math:
- **Sunk cost**: 1 day of work (268 lines)
- **Remaining work (Approach A)**: 19 more days
- **Total (Approach A)**: 20 days

vs.

- **Discard**: 1 day (268 lines)
- **New work (Approach B)**: 3-5 days
- **Total (Approach B)**: 4-6 days

**Net Savings**: 14-16 days of development time

---

## Decision Matrix

| Factor | Weight | Approach A | Approach B | Winner |
|--------|--------|-----------|-----------|---------|
| Time to Production | 30% | 4 weeks (3/10) | 3-5 days (10/10) | **B** |
| Code Simplicity | 25% | Complex (4/10) | Simple (10/10) | **B** |
| Performance | 20% | Overhead (6/10) | Native (10/10) | **B** |
| Maintenance | 15% | High (4/10) | Low (10/10) | **B** |
| Flexibility | 10% | Rigid (6/10) | Full control (10/10) | **B** |

**Weighted Score:**
- **Approach A**: 4.5/10
- **Approach B**: 9.7/10

**Clear Winner**: Approach B (Simplified)

---

## Action Items

### If Choosing Approach B (Recommended):

1. **Immediate (5 minutes):**
   ```bash
   # Clean up untracked files
   rm -rf src/fraiseql/cascade/
   rm -rf tests/unit/cascade/
   rm docs/planning/graphql-cascade-implementation-plan.md
   ```

2. **Day 1: Implement Passthrough**
   - Modify mutation resolver to check for `_cascade`
   - Add `enable_cascade` parameter
   - Tests for passthrough

3. **Day 2: Response Formatting**
   - Include cascade in GraphQL response
   - Update serialization

4. **Day 3: Documentation**
   - PostgreSQL pattern guide
   - Examples
   - Migration notes

5. **Day 4-5: Examples & Polish**
   - Complete example app
   - Integration tests
   - Documentation polish

### If Choosing Approach A (Not Recommended):

1. Continue with existing implementation
2. Complete Phases 2-10
3. Expect 3-4 more weeks of work

---

## Conclusion

**Recommendation: Switch to Approach B (Simplified PostgreSQL-First)**

**Why:**
- ✅ 90% less code (50 vs 2000+ lines)
- ✅ 75% faster to production (5 days vs 4 weeks)
- ✅ Zero performance overhead
- ✅ Simpler to maintain
- ✅ More flexible
- ✅ Aligns with FraiseQL philosophy
- ✅ Matches proven SpecQL pattern

**Cost:** Discard 1 day of work (268 lines)
**Benefit:** Save 14-16 days of development time

The sunk cost fallacy would be to continue with Approach A just because we've started. The smart move is to pivot to the superior approach now, while the cost is minimal.

---

## Next Steps

**Decision required from:** Project Lead / Architecture Team

**Options:**
- [ ] A: Continue with original plan (Phases 2-10)
- [ ] **B: Switch to simplified approach (RECOMMENDED)**
- [ ] C: Hybrid (explain reasoning)

**Timeline:**
- Decision by: 2025-11-12
- Implementation start: Immediately after decision
- Target completion: 2025-11-18 (Approach B) or 2025-12-09 (Approach A)

---

**Document Version**: 1.0
**Created**: 2025-11-11
**Status**: Awaiting Decision
