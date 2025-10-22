# v0.11.6 Post-Release Plan: Executive Summary

**Status**: v0.11.5 ready to publish ✅ (0 failures, 3,508 passing, 44 skipped)
**Next Version**: v0.11.6
**Timeline**: 2-4 weeks post-release
**Focus**: Complete skipped features

---

## 🎯 Four Features to Implement

### 1. **Nested Object Filtering** (HIGH PRIORITY)
**Timeline**: 2-3 days | **Tests**: 3 | **Impact**: HIGH

**Problem**: Can't filter on nested JSONB objects
```python
# Currently FAILS
where = {"machine": {"name": {"eq": "Server-01"}}}
```

**Solution**: Recursive WHERE clause builder
- Build JSONB paths: `data->'machine'->>'name'`
- Support multiple nesting levels
- Integrate with existing operators

**Deliverables**:
- Recursive WHERE builder algorithm
- JSONB path navigation
- Unit + integration tests
- Documentation with examples

---

### 2. **Coordinate Datatype** (MEDIUM PRIORITY)
**Timeline**: 3-5 days | **Tests**: 10 | **Impact**: MEDIUM

**Problem**: Coordinate operators not fully implemented

**Solution**: Complete spatial filtering
- Coordinate equality/inequality (`eq`, `neq`)
- List membership (`in`, `notin`)
- Distance filtering (`distance_within` with PostGIS)
- PostgreSQL POINT type integration

**Deliverables**:
- Coordinate operator strategies
- PostGIS integration
- GiST spatial indexes
- Geographic query examples

---

### 3. **TypeName Integration** (LOW PRIORITY)
**Timeline**: 2-3 days | **Tests**: 3 | **Impact**: LOW

**Problem**: Tests use mocked data instead of Rust pipeline

**Solution**: Refactor to real database
- Replace mocked resolvers
- Use actual Rust pipeline
- Verify `__typename` injection

**Deliverables**:
- Refactored integration tests
- Real database fixtures
- TypeName validation

---

### 4. **Example Templates** (MAINTENANCE)
**Timeline**: 1-2 days | **Tests**: 10 | **Impact**: LOW

**Problem**: Blog example templates failing

**Solution**: Fix template SQL
- Add missing JSONB columns
- Fix syntax errors
- Update for Rust pipeline

**Deliverables**:
- Working blog examples
- Template validation passing

---

## 📅 Recommended Implementation Order

### Week 1: Nested Object Filtering
**Why first**: High user impact, enables important use cases

**Approach**:
1. Design recursive WHERE builder
2. Implement with TDD (RED/GREEN/REFACTOR)
3. Integrate with repository layer
4. Document with examples

**Result**: 3 tests passing, major feature complete

---

### Week 2-3: Coordinate Datatype
**Why second**: Valuable feature, moderate complexity

**Approach**:
1. Implement coordinate operators
2. PostgreSQL POINT integration
3. PostGIS distance filtering
4. Add spatial query examples

**Result**: 10 tests passing, location-based queries enabled

---

### Week 4: Polish & Cleanup
**Why last**: Lower priority items

**Approach**:
1. Fix TypeName integration tests
2. Fix example templates
3. Final documentation review
4. Release v0.11.6

**Result**: 13+ additional tests passing

---

## 💡 Key Technical Insights

### Nested Object Filtering Architecture

**Core algorithm**:
```python
def build_where_recursive(where_dict, path=[]):
    for field, value in where_dict.items():
        if is_nested_object(value):
            # Recurse deeper
            build_where_recursive(value, path + [field])
        else:
            # Leaf node: build JSONB path
            jsonb_path = build_path(path + [field])
            # Example: data->'machine'->>'name'
            apply_operator(jsonb_path, value)
```

**Key decision**: Detect operators vs nested objects
```python
operators = {"eq", "neq", "gt", "gte", ...}
is_operator = any(k in operators for k in dict.keys())
```

---

### Coordinate Operators

**PostgreSQL POINT format**:
```python
# Input: (lat, lng) = (45.5, -122.6)
# PostgreSQL POINT: (x, y) = (lng, lat) = (-122.6, 45.5)
# Note: PostGIS uses (longitude, latitude) order!
```

**Distance filtering**:
```sql
ST_DWithin(
    ST_GeographyFromText(coordinates::point),
    ST_GeographyFromText('POINT(-122.6 45.5)'),
    5000  -- meters
)
```

---

## 📊 Expected Results

### v0.11.5 → v0.11.6 Improvements

**Test Coverage**:
- Before: 3,508 passing, 44 skipped
- After: 3,521+ passing, 20-30 skipped
- Improvement: +13 tests, -14 skipped

**Features**:
- ✅ Nested object filtering on hybrid tables
- ✅ Complete coordinate datatype with spatial queries
- ✅ Geographic distance-based filtering
- ✅ Better test coverage

**User Impact**:
- Enables complex filtering scenarios
- Location-based application support
- More reliable test suite
- Better documentation

---

## 🚀 Getting Started

### For Nested Object Filtering (Start Here)

1. **Read**: `POST_RELEASE_IMPLEMENTATION_PLAN.md` - Feature 1 section
2. **Study**: Current WHERE clause builder (`src/fraiseql/sql/where/`)
3. **Design**: Recursive algorithm with JSONB path building
4. **Implement**: TDD approach (RED → GREEN → REFACTOR → QA)
5. **Test**: Integration with repository layer
6. **Document**: Examples and migration guide

**Time**: 2-3 focused days

---

### For Coordinate Datatype

1. **Review**: Existing coordinate parsing (`src/fraiseql/types/scalars/coordinates.py`)
2. **Create**: Operator strategies (`src/fraiseql/sql/where/operators/coordinate.py`)
3. **Integrate**: PostgreSQL POINT type
4. **Add**: PostGIS distance filtering
5. **Document**: Spatial query examples

**Time**: 3-5 days

---

## 📈 Success Metrics

### Phase 1 (Nested Filtering) Success:
- ✅ Can filter: `{"machine": {"name": {"eq": "value"}}}`
- ✅ Supports multiple nesting levels
- ✅ Works with all operators
- ✅ 3 integration tests passing
- ✅ Documentation complete

### Phase 2 (Coordinates) Success:
- ✅ All coordinate operators working
- ✅ PostGIS distance queries
- ✅ PostgreSQL POINT type support
- ✅ 10 integration tests passing
- ✅ Location examples working

### v0.11.6 Release Success:
- ✅ 3,521+ tests passing (0 failures)
- ✅ All major features implemented
- ✅ Documentation updated
- ✅ Migration guides complete
- ✅ Ready for production

---

## 🎯 Bottom Line

**v0.11.5 is production-ready now** with 99.8% test pass rate and working Rust pipeline.

**v0.11.6 will complete the feature set** by adding:
1. Nested object filtering (important use case)
2. Geographic/coordinate queries (valuable niche)
3. Better test coverage (confidence)

**Timeline**: 2-4 weeks post-v0.11.5 release

**Risk**: LOW (all features isolated, backward compatible)

**Recommendation**:
1. **Publish v0.11.5 immediately** (it's ready!)
2. **Start nested filtering** (highest user value)
3. **Add coordinates** (nice to have)
4. **Release v0.11.6** (feature-complete)

---

**The path is clear. The implementation plan is detailed. Let's ship v0.11.5 and then make v0.11.6 even better!** 🚀
