# Test Organization Plan for v2.0

**Status**: Implementation Plan
**Created**: January 8, 2026
**Timeline**: Phased approach
**Goal**: Consolidate ~30 root-level test files into organized directories

---

## Current State

### Problem

30 test files currently at `/tests/` root level without clear functional grouping:

```
tests/
├── test_apq_*.py           # 5+ files - APQ feature
├── test_nested_array_*.py  # 3+ files - Array filtering
├── test_subscriptions_*.py # 4+ files - WebSocket subscriptions
├── test_dataloader_*.py    # 3+ files - Data loading
├── test_mutation_*.py      # 4+ files - Mutation patterns
├── test_caching_*.py       # 3+ files - Query caching
└── [other test files]      # 5+ files - Miscellaneous
```

### Impact

- ❌ Hard to navigate test suite
- ❌ Unclear test classification
- ❌ Difficult to run tests by feature
- ❌ No clear unit vs integration distinction

---

## Target State (v2.0)

All tests organized by feature + type:

```
tests/
├── unit/
│   ├── apq/                        # APQ unit tests
│   │   ├── test_parser.py
│   │   ├── test_cache_key.py
│   │   └── test_validation.py
│   ├── array_filtering/
│   ├── subscriptions/
│   ├── dataloader/
│   ├── caching/
│   └── ...
├── integration/
│   ├── apq/                        # APQ integration tests
│   │   └── test_apq_with_database.py
│   ├── subscriptions/
│   └── ...
└── [system, regression, chaos, fixtures]
```

---

## Migration Plan

### Phase 1: Categorize Root Files (Week 1)

Audit all root-level test files and categorize:

| File | Category | Target Directory | Priority |
|------|----------|-------------------|----------|
| `test_apq_*.py` (5 files) | APQ feature | `unit/apq/` | High |
| `test_nested_array_*.py` (3) | Array filtering | `unit/array_filtering/` | High |
| `test_subscriptions_*.py` (4) | Subscriptions | `unit/subscriptions/` | High |
| `test_dataloader_*.py` (3) | Data loading | `unit/dataloader/` | High |
| `test_mutation_*.py` (4) | Mutations | `unit/mutations/` | Medium |
| `test_caching_*.py` (3) | Caching | `integration/caching/` | Medium |
| Other (5) | TBD | Case-by-case | Medium |

### Phase 2: Create Target Directories (Week 1)

Create directory structure before moving files:

```bash
mkdir -p tests/unit/{apq,array_filtering,subscriptions,dataloader,mutations}
mkdir -p tests/integration/{apq,subscriptions,caching}
```

### Phase 3: Classify Tests (Week 2)

For each root-level test file:

1. **Determine test type**:
   - **Unit**: No external dependencies, fast
   - **Integration**: Uses database, slower
   - **Both**: Move to appropriate directory

2. **Determine feature area**:
   - Core GraphQL
   - Specific feature (APQ, subscriptions)
   - Performance (caching, dataloader)

3. **Add pytest markers** if missing:
   ```python
   @pytest.mark.unit
   @pytest.mark.apq
   def test_apq_parsing():
       ...
   ```

### Phase 4: Move Files (Week 2-3)

Move tests with git to preserve history:

```bash
git mv tests/test_apq_*.py tests/unit/apq/
git mv tests/test_subscriptions_*.py tests/unit/subscriptions/
# ... etc
```

### Phase 5: Update Imports (Week 3)

Fix relative imports in moved test files:

```python
# Before (when file at tests/test_apq.py)
from conftest import fixture

# After (when file at tests/unit/apq/test_apq.py)
from tests.fixtures import fixture
# OR absolute import
from fraiseql.apq import APQParser
```

### Phase 6: Verify & Test (Week 3)

Ensure all tests still pass:

```bash
pytest tests/unit/apq/          # Run specific feature tests
pytest tests/unit/               # Run all unit tests
pytest                          # Run entire suite
```

### Phase 7: Document (Week 4)

Update test documentation:
- Update `tests/README.md` with new structure
- Add marker documentation
- Create feature-specific test guides

---

## File-by-File Migration

### APQ Tests

**Files to migrate**:
- `test_apq_parser.py`
- `test_apq_caching.py`
- `test_apq_validation.py`
- `test_apq_persistence.py`
- `test_apq_multiquery.py`

**Target directories**:
```
tests/unit/apq/
├── test_parser.py
├── test_cache_key.py
├── test_validation.py
└── test_persistence.py

tests/integration/apq/
└── test_apq_with_database.py
```

**Rationale**: APQ is a cohesive feature, benefits from grouped tests

---

### Nested Array Filtering Tests

**Files to migrate**:
- `test_nested_array_filtering.py`
- `test_nested_array_where_generation.py`
- `test_nested_array_optimization.py`

**Target directories**:
```
tests/unit/array_filtering/
├── test_basic_filtering.py
├── test_where_generation.py
└── test_optimization.py
```

**Rationale**: Part of SQL generation feature

---

### Subscription Tests

**Files to migrate**:
- `test_subscriptions_basic.py`
- `test_subscriptions_protocol.py`
- `test_subscriptions_lifecycle.py`
- `test_subscriptions_auth.py`

**Target directories**:
```
tests/unit/subscriptions/
├── test_basic.py
├── test_protocol.py
└── test_lifecycle.py

tests/integration/subscriptions/
└── test_websocket_with_database.py
```

**Rationale**: Subscriptions are optional advanced feature

---

### Dataloader Tests

**Files to migrate**:
- `test_dataloader_batching.py`
- `test_dataloader_caching.py`
- `test_dataloader_errors.py`

**Target directories**:
```
tests/unit/dataloader/
├── test_batching.py
├── test_caching.py
└── test_errors.py
```

**Rationale**: Optimization feature, clear cohesion

---

### Mutation Tests

**Files to migrate**:
- `test_mutation_basic.py`
- `test_mutation_validation.py`
- `test_mutation_errors.py`
- `test_mutation_cascading.py`

**Target directories**:
```
tests/unit/mutations/
├── test_basic.py
├── test_validation.py
├── test_errors.py
└── test_cascading.py
```

**Rationale**: Part of core GraphQL feature

---

### Caching Tests

**Files to migrate**:
- `test_caching_strategy.py`
- `test_caching_invalidation.py`
- `test_caching_postgres.py`

**Target directories**:
```
tests/integration/caching/
├── test_result_caching.py
├── test_cache_invalidation.py
└── test_postgres_backend.py
```

**Rationale**: Requires database, integration tests

---

## Test Marker Assignment

Ensure all migrated tests have proper markers:

```python
# APQ tests
@pytest.mark.unit
@pytest.mark.apq

# Subscription tests
@pytest.mark.unit
@pytest.mark.subscriptions

# Integration tests
@pytest.mark.integration
@pytest.mark.caching
@pytest.mark.database
```

---

## Running Tests by Feature

After migration, test by feature easily:

```bash
# All APQ tests
pytest -m apq

# APQ unit tests only
pytest -m "apq and unit"

# Subscription integration tests
pytest -m "subscriptions and integration"

# All unit tests
pytest -m unit

# Specific directory
pytest tests/unit/apq/
```

---

## Import Path Updates

### Example: APQ test migration

**Before** (at `tests/test_apq_parser.py`):
```python
from conftest import auth_client, graphql_client
from fraiseql.middleware.apq import APQParser
```

**After** (at `tests/unit/apq/test_parser.py`):
```python
from tests.fixtures.auth import auth_client
from tests.fixtures.graphql import graphql_client
from fraiseql.middleware.apq import APQParser
```

Or use absolute imports:
```python
from fraiseql.middleware.apq import APQParser

# Fixtures from root conftest still work
def test_parser(graphql_client):
    ...
```

---

## Verification Checklist

After each file migration:

- [ ] File moved to correct directory
- [ ] All imports updated
- [ ] Tests still pass (`pytest tests/unit/[feature]/`)
- [ ] Pytest markers present
- [ ] Git history preserved (`git log -- tests/[old/path]`)

---

## Timeline

| Week | Phase | Tasks |
|------|-------|-------|
| 1 | Categorize & Prepare | Audit files, create directories |
| 2 | Classify & Move | Add markers, move files |
| 3 | Verify | Fix imports, run tests |
| 4 | Document | Update docs, finalize |

---

## Rollback Plan

If issues occur during migration:

```bash
# Revert individual file moves
git checkout HEAD tests/test_file.py
git rm tests/unit/feature/test_file.py

# Or revert entire migration
git revert <commit-hash>
```

---

## Benefits After Migration

✅ **Better navigation**: Feature-based organization
✅ **Easier testing**: Run tests by feature (`pytest -m apq`)
✅ **Clear structure**: Unit vs integration distinction
✅ **Scalability**: Easier to add new features
✅ **Maintenance**: Clearer ownership boundaries

---

## Related Documentation

- **Test organization**: `tests/README.md`
- **Code standards**: `docs/CODE_ORGANIZATION_STANDARDS.md`
- **Main organization**: `docs/ORGANIZATION.md`
- **CI/CD integration**: `.github/workflows/`

---

## Questions?

**Q: Will this affect test performance?**
A: No, pytest runs all tests regardless of directory structure.

**Q: Do I need to update conftest.py?**
A: No, root conftest is still used. Feature-specific conftest can be added.

**Q: Can I add new tests to correct locations immediately?**
A: Yes! New code should follow new structure immediately.

**Q: What about legacy test files?**
A: Move them as part of this plan or archive in `.archive/`.

---

**Last Updated**: January 8, 2026
**Phase**: Planning
**Next Step**: Week 1 audit and categorization
