# Next Improvement Opportunities
**Date**: January 4, 2026
**Status**: 🟡 PLANNING - Ready for selection
**Based On**: Phase 3 Codebase Improvements plan (26 issues identified)

---

## 📊 Overview

We've completed **Phase 1 (Quick Wins)** with all 3 improvements. There are **16 additional improvements** organized into **Phase 2 (Important)** and **Phase 3 (Polish)** that can be implemented.

**Total Estimated Effort**: 18-29 hours for all remaining improvements

---

## 🚀 Phase 2: Important Improvements (13-19 hours)

### Phase 2.1: Create Type Stubs for IDE Autocompletion ⭐
**Priority**: 6 | **Impact**: MEDIUM | **Effort**: HIGH (4-6 hours)
**Difficulty**: Medium

**What It Does**:
- Generate `.pyi` stub files for major modules
- Enables IDE autocompletion and type checking
- Improves developer experience significantly

**Current Status**:
- ✅ `__init__.pyi` exists (partial)
- ✅ `fastapi.pyi` exists
- ✅ `repository.pyi` exists
- ❌ Missing: `db.py`, `decorators.py`, `types/fraise_type.py`, `caching/`, `enterprise/rbac/`, `auth/`

**Benefits**:
- IDE autocompletion for all major APIs
- Type checking in user code
- Better documentation in IDEs
- Reduced runtime type errors

**Implementation Approach**:
1. Create stub file for each major module
2. Document function signatures and return types
3. Run type checker to validate stubs
4. Add to pre-commit validation

**Estimated Time**: 4-6 hours

---

### Phase 2.2: Document Advanced Features (3-4 hours)
**Priority**: 7 | **Impact**: MEDIUM | **Effort**: HIGH
**Difficulty**: Medium

**What It Does**:
Create comprehensive guides for enterprise features

**Features to Document** (5 guides):

1. **Caching** (`docs/advanced/caching.md`)
   - Setup CachedRepository
   - Auto-invalidation with CASCADE rules
   - Cache key strategies
   - Performance benefits
   - ~2 hours

2. **RBAC** (`docs/advanced/rbac.md`)
   - Row-level security setup
   - Constraint resolution
   - Conflict strategies
   - Real-world examples
   - ~1.5 hours

3. **Audit Logging** (`docs/advanced/audit.md`)
   - AuditLogger setup
   - Event tracking
   - Query analysis
   - Security considerations
   - ~1 hour

4. **APQ** (`docs/advanced/apq.md`)
   - Automatic Persistent Queries
   - Performance benefits (70% bandwidth reduction)
   - Client integration
   - ~1 hour

5. **Dataloader** (`docs/advanced/dataloader.md`)
   - Batch loading pattern
   - N+1 prevention
   - Pagination with dataloader
   - ~1 hour

**Benefits**:
- Developers understand enterprise features
- Reduces support questions
- Enables advanced use cases
- Improves adoption

**Estimated Time**: 3-4 hours

---

### Phase 2.3: Type-Safe GraphQL Context (4-6 hours) ⭐⭐
**Priority**: 9 | **Impact**: HIGH | **Effort**: MEDIUM
**Difficulty**: Medium

**What It Does**:
Replace unsafe `info.context["db"]` with type-safe `context.db`

**Current Problem**:
```python
@fraiseql.query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]  # ❌ Not type-safe, no IDE help
    user = await db.find_one("users", {"id": id})
```

**Solution**:
```python
from fraiseql.types.context import GraphQLContext

@fraiseql.query
async def get_user(info: GraphQLResolveInfo, id: UUID) -> User:
    context: GraphQLContext = info.context
    user = await context.db.find_one("users", {"id": id})  # ✅ Type-safe!
```

**Implementation**:

1. **Create `src/fraiseql/types/context.py`**:
   ```python
   @dataclass
   class GraphQLContext(Generic[T]):
       db: CQRSRepository
       user: UserContext | None = None
       request: Any | None = None
       response: Any | None = None
       _extras: dict[str, Any] = None
   ```

2. **Update FastAPI integration** to use typed context

3. **Create helper** for custom context building:
   ```python
   def build_context(db, user=None, **extras) -> GraphQLContext:
       """Build type-safe GraphQL context."""
   ```

4. **Add to `__init__.py`** exports

5. **Update documentation** with migration guide

**Benefits**:
- IDE autocompletion for context access
- Type errors caught at development time
- Cleaner, more readable code
- Reduces runtime errors

**Migration Path**:
- Add GraphQLContext as optional
- Document migration path
- Provide deprecation warning for old pattern
- Mark old pattern as legacy

**Estimated Time**: 4-6 hours

---

### Phase 2.4: Improve Error Messages (2-3 hours)
**Priority**: 8 | **Impact**: MEDIUM | **Effort**: LOW
**Difficulty**: Low

**What It Does**:
Make error messages more helpful and actionable

**Improvements**:

1. **Schema Composition Errors**
   - Add context about which type failed
   - Show validation rules violated
   - Suggest fixes

2. **WHERE Clause Errors**
   - Show valid operators for field type
   - Explain why operator is invalid
   - Provide examples

3. **Type Definition Errors**
   - Explain what decorators are required
   - Show correct usage patterns
   - Link to documentation

4. **Missing Pool Errors**
   - Explain which pool to use and why
   - Show import statement
   - Link to database selection guide

**Example**:
```
❌ Current:
ValueError: Invalid operator 'contains' for field 'age'

✅ Improved:
ValueError: Invalid operator 'contains' for field 'age' (type: int)
Allowed operators for int: eq, neq, gt, gte, lt, lte, in, notin, isnull
Did you mean 'eq' or 'in'?
See: docs/filtering.md#operators
```

**Benefits**:
- Faster debugging
- Less documentation lookups
- Better user experience
- Self-documenting code

**Estimated Time**: 2-3 hours

---

## 💎 Phase 3: Polish & Performance (5-10 hours)

### Phase 3.1: Performance Optimizations
**Priority**: 5 | **Impact**: LOW | **Effort**: MEDIUM
**Difficulty**: Medium

**Optimizations** (2-3 hours):

1. **Memoize Type Registry Lookups**
   - Cache field type lookups
   - Reduce repeated introspection
   - ~0.5h

2. **Improve Null Response Cache**
   - Better cache key strategy
   - Higher hit rate
   - ~1h

3. **Optimize Schema Registry Singleton**
   - Lazy initialization
   - Reduce startup time
   - ~1.5h

**Benefits**:
- Faster schema initialization
- Lower memory usage
- Reduced CPU per query

---

### Phase 3.2: Error Class Hierarchy
**Priority**: 4 | **Impact**: LOW | **Effort**: LOW
**Difficulty**: Low

**What It Does**:
Standardize error class structure (1-2 hours)

**Changes**:
- Base `FraiseQLError` class
- Specific error subclasses
- Consistent `__str__` methods
- Better error categorization

**Benefits**:
- Easier error handling
- Better error filtering
- More professional error messages

---

### Phase 3.3: Naming Consistency
**Priority**: 3 | **Impact**: LOW | **Effort**: MEDIUM
**Difficulty**: Low

**What It Does**:
Ensure consistent naming conventions (2-3 hours)

**Areas**:
- Function naming (`create_*` vs `make_*`)
- Variable naming (`db` vs `connection`)
- Module naming (consistency)

**Benefits**:
- More predictable API
- Easier to remember function names
- Better code readability

---

## 📈 Implementation Roadmap

### Recommended Order (By Value):

1. **Phase 2.3: Type-Safe Context** (4-6h)
   - Highest impact on developer experience
   - Improves IDE support dramatically
   - Enables better type checking

2. **Phase 2.1: Type Stubs** (4-6h)
   - Complements type-safe context
   - Enables IDE autocompletion
   - Reduces runtime errors

3. **Phase 2.2: Advanced Features Docs** (3-4h)
   - High value for advanced users
   - Reduces support burden
   - Enables enterprise features

4. **Phase 2.4: Error Messages** (2-3h)
   - Quick win, immediate benefit
   - Improves debugging
   - Better user experience

5. **Phase 3 Items** (5-10h)
   - Polish and refinement
   - Performance gains
   - Code quality improvements

---

## 🎯 Quick Selection Guide

**If you want to focus on:**

- **Developer Experience** → Implement Phase 2.3 + Phase 2.1 (8-12h)
- **Enterprise Features** → Implement Phase 2.2 (3-4h)
- **Code Quality** → Implement Phase 3 items (5-10h)
- **Quick Wins** → Implement Phase 2.4 + Phase 3.1 (4-6h)
- **Everything** → All 16 improvements (18-29h)

---

## 📊 Effort vs Impact Matrix

```
High Impact, Low Effort (Do First):
  ⭐⭐⭐ Phase 2.4 (Error Messages) - 2-3h
  ⭐⭐⭐ Phase 2.3 (Type-Safe Context) - 4-6h
  ⭐⭐ Phase 2.1 (Type Stubs) - 4-6h

High Impact, Medium Effort (Do Second):
  ⭐⭐ Phase 2.2 (Feature Docs) - 3-4h
  ⭐ Phase 3.1 (Performance) - 2-3h

Low Impact, Low Effort (Polish):
  Phase 3.2 (Error Classes) - 1-2h
  Phase 3.3 (Naming) - 2-3h
```

---

## 🔄 Implementation Patterns

All improvements follow these patterns:

1. **Non-Breaking**: All additive, no breaking changes
2. **Backward Compatible**: Old patterns still work
3. **Well-Documented**: Each change includes docs
4. **Test Coverage**: New code is tested
5. **Reviewable**: Small, focused PRs

---

## ✅ Success Criteria for Each Phase

**Phase 2.1 Success**:
- [ ] 6+ type stub files created
- [ ] IDE autocompletion works for major APIs
- [ ] 95%+ function signatures covered
- [ ] No type checking errors

**Phase 2.2 Success**:
- [ ] 5 feature guides published
- [ ] 500+ lines of documentation added
- [ ] All examples tested and working
- [ ] Links validated

**Phase 2.3 Success**:
- [ ] GraphQLContext class works
- [ ] FastAPI integration updated
- [ ] IDE shows proper types
- [ ] All tests pass
- [ ] Migration guide documented

**Phase 2.4 Success**:
- [ ] 10+ error messages improved
- [ ] Error suggestions are helpful
- [ ] Documentation links work
- [ ] Users report better debugging

---

## 🚀 Getting Started

### To Implement Phase 2.3 (Type-Safe Context) First:

```bash
# 1. Create the context module
touch src/fraiseql/types/context.py

# 2. Implement GraphQLContext class with:
#    - dataclass decorator
#    - db: CQRSRepository
#    - user: UserContext | None
#    - request, response fields
#    - _extras dict for flexibility

# 3. Add to exports
# Edit src/fraiseql/__init__.py
# Add: "GraphQLContext" to imports and __all__

# 4. Update FastAPI integration
# Edit src/fraiseql/fastapi/app.py
# Use GraphQLContext in context_factory

# 5. Add tests
# tests/unit/types/test_context.py
# Test: TypedContext creation and access

# 6. Add migration guide
# docs/migration/type-safe-context.md
```

### To Implement Phase 2.1 (Type Stubs):

```bash
# For each module:
# 1. Create src/fraiseql/module.pyi
# 2. Copy function signatures from .py file
# 3. Add return types and parameter types
# 4. Run: python -m mypy src/fraiseql/module.py
# 5. Fix any type errors
```

---

## 📞 Decision Points

**Questions to answer before starting:**

1. **Which improvements matter most to you?**
   - Type safety → Phase 2.3 + 2.1
   - Better docs → Phase 2.2
   - Quick wins → Phase 2.4
   - Everything → All phases

2. **How much time do you have?**
   - <5h → Phase 2.4 only
   - 5-10h → Phase 2.4 + 2.1
   - 10-15h → Phase 2.3 + 2.1 + 2.2
   - 15+h → All improvements

3. **Which would help your use case most?**

---

## 🎓 Learning Opportunities

Each improvement teaches valuable skills:

- **Phase 2.1**: Python type stubs and static typing
- **Phase 2.2**: Technical documentation writing
- **Phase 2.3**: Typed Python dataclasses and generics
- **Phase 2.4**: Error message design and user experience
- **Phase 3**: Performance analysis and optimization

---

## 📚 Related Documentation

- **Phase 3 Full Plan**: `.phases/CODEBASE-IMPROVEMENTS-2026-01-04.md`
- **Session Summary**: `.phases/SESSION-COMPLETION-2026-01-04.md`
- **Cleanup Plan**: `.phases/REPOSITORY-CLEANUP-2026-01-04.md`

---

## 🎉 Summary

We have 16 additional improvements organized into 3 phases:

| Phase | Improvements | Hours | Priority |
|-------|--------------|-------|----------|
| **2** | 4 important items | 13-19 | **HIGH** |
| **3** | 3 polish items | 5-10 | **MEDIUM** |
| **Total** | 16 improvements | 18-29 | - |

**Recommended Next Step**: Phase 2.3 (Type-Safe Context) - highest ROI

---

*Last Updated: January 4, 2026*
*Continuation of: Phase 3 Codebase Improvements Plan*
*Status: Ready for implementation*
