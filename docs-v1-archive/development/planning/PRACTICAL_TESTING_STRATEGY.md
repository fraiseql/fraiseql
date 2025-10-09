# FraiseQL Special Types: Practical Testing Strategy

## 🚨 Reality Check: 14,400 Tests is Insane

**Problem**: The bulletproof plan calls for 14,400 test combinations, which would:

- Take **hours to run** on every commit
- **Block development velocity** completely
- **Overwhelm CI/CD pipelines**
- Make debugging nearly impossible

**Solution**: Implement a **smart tiered testing strategy** with selective execution.

---

# 🎯 Tiered Testing Approach

## Tier 1: Core Smoke Tests (Always Run) ⚡
**Target**: < 30 seconds execution time
**Frequency**: Every commit, PR, local development

```python
@pytest.mark.core
class CoreSpecialTypeTests:
    """Essential tests that must pass for basic functionality."""

    # One representative test per type/operator combination
    test_cases = [
        # Network (4 core tests)
        ("ip_address", "eq", "8.8.8.8", "jsonb_flat"),
        ("ip_address", "isPrivate", True, "jsonb_flat"),
        ("ip_address", "isPublic", True, "jsonb_flat"),
        ("ip_address", "inSubnet", "192.168.0.0/16", "jsonb_flat"),

        # LTree (3 core tests)
        ("path", "eq", "top.middle.bottom", "jsonb_flat"),
        ("path", "ancestor_of", "top.middle.bottom", "jsonb_flat"),
        ("path", "matches_lquery", "top.*", "jsonb_flat"),

        # DateRange (3 core tests)
        ("period", "eq", "[2024-01-01,2024-12-31)", "jsonb_flat"),
        ("period", "contains_date", "2024-06-15", "jsonb_flat"),
        ("period", "overlaps", "[2024-06-01,2024-06-30)", "jsonb_flat"),

        # MacAddress (2 core tests)
        ("mac", "eq", "00:11:22:33:44:55", "jsonb_flat"),
        ("mac", "in", ["00:11:22:33:44:55", "AA:BB:CC:DD:EE:FF"], "jsonb_flat"),
    ]

    # Total: 12 core tests covering basic functionality
```

## Tier 2: Regression Tests (CI/CD Only) 🔄
**Target**: < 5 minutes execution time
**Frequency**: Pre-merge, nightly builds

```python
@pytest.mark.regression
class RegressionSpecialTypeTests:
    """Tests covering known failure scenarios and edge cases."""

    # Focus on previously broken combinations
    previously_broken = [
        # Known network issues
        ("ip_address", "eq", "8.8.8.8", "jsonb_nested"),
        ("ip_address", "isPrivate", True, "materialized_view"),

        # Known ltree issues
        ("path", "descendant_of", "top", "jsonb_nested"),

        # Known daterange issues
        ("period", "adjacent", "[2024-06-30,2024-12-31)", "jsonb_flat"),

        # Add more as issues are discovered and fixed
    ]

    # Malformed input testing (one per type)
    malformed_input_tests = [
        ("ip_address", "eq", "invalid.ip.address"),
        ("path", "ancestor_of", "invalid..path"),
        ("period", "contains_date", "invalid-date"),
        ("mac", "eq", "invalid:mac:format"),
    ]

    # Total: ~50 regression tests
```

## Tier 3: Comprehensive Matrix (Release Only) 🏁
**Target**: < 2 hours execution time
**Frequency**: Pre-release, weekly comprehensive validation

```python
@pytest.mark.comprehensive
@pytest.mark.slow
class ComprehensiveSpecialTypeTests:
    """Full matrix testing - only run before releases."""

    # This is where the 14,400 combinations live
    # But we're smarter about it:

    @pytest.mark.parametrize("special_type", ["Network", "LTree", "DateRange", "MacAddress"])
    @pytest.mark.parametrize("storage_pattern", ["jsonb_flat", "jsonb_nested", "direct_column"])
    @pytest.mark.parametrize("postgres_version", ["12", "15", "17"])  # Sample versions
    def test_comprehensive_matrix(self, special_type, storage_pattern, postgres_version):
        # Smart sampling: not every combination, but representative coverage
        pass

    # Reduced to ~500 strategic combinations instead of 14,400
```

## Tier 4: Stress & Edge Cases (On-Demand) 💥
**Target**: Can run for hours
**Frequency**: Manual trigger, performance validation

```python
@pytest.mark.stress
@pytest.mark.manual
class StressSpecialTypeTests:
    """Heavy testing for performance and edge cases."""

    def test_large_dataset_performance(self):
        # 10K+ records, complex queries
        pass

    def test_all_postgresql_versions(self):
        # Every PostgreSQL version 12-17
        pass

    def test_pathological_inputs(self):
        # Every possible malformed input
        pass
```

---

# 🏃‍♂️ Execution Strategy

## pytest.ini Configuration
```ini
[tool:pytest]
markers =
    core: Essential tests that always run (< 30s)
    regression: Known issue tests for CI/CD (< 5min)
    comprehensive: Full matrix for releases (< 2hr)
    stress: Performance and edge case testing (manual)
    manual: Tests that require manual trigger
    slow: Tests that take significant time

# Default: only run core tests
addopts = -m "not slow and not manual"
```

## Makefile Targets
```makefile
# Default - core tests only (30 seconds)
test:
	pytest -m "core"

# CI/CD pipeline (5 minutes)
test-ci:
	pytest -m "core or regression"

# Pre-release validation (2 hours)
test-release:
	pytest -m "not manual"

# Full nuclear option (manual trigger)
test-nuclear:
	pytest --no-cov -x
```

## GitHub Actions Integration
```yaml
name: Special Types Testing

on: [push, pull_request]

jobs:
  core-tests:
    runs-on: ubuntu-latest
    steps:

      - name: Core Tests (Fast)
        run: make test  # 30 seconds

  regression-tests:
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:

      - name: Regression Tests
        run: make test-ci  # 5 minutes

  comprehensive-tests:
    runs-on: ubuntu-latest
    if: contains(github.ref, 'release')
    steps:

      - name: Comprehensive Tests
        run: make test-release  # 2 hours
```

---

# 🎯 Smart Test Selection Strategies

## Strategy 1: Risk-Based Sampling
```python
# Focus on highest-risk combinations first
HIGH_RISK_COMBINATIONS = [
    # JSONB + Network (known broken)
    ("Network", "jsonb_flat", ["eq", "isPrivate", "isPublic"]),

    # LTree + Complex patterns (potential issues)
    ("LTree", "jsonb_nested", ["ancestor_of", "matches_lquery"]),

    # DateRange + Edge cases (boundary issues)
    ("DateRange", "all_patterns", ["overlaps", "adjacent"]),
]

# Sample 20% of medium-risk, 5% of low-risk
```

## Strategy 2: Rotating Test Sets
```python
# Different subset each day to eventually cover everything
MONDAY_TESTS = ["Network", "MacAddress"]
TUESDAY_TESTS = ["LTree", "DateRange"]
WEDNESDAY_TESTS = ["jsonb_flat", "jsonb_nested"]
# etc...

@pytest.mark.skipif(
    datetime.now().weekday() != 0,  # Monday
    reason="Monday tests only"
)
def test_monday_subset():
    pass
```

## Strategy 3: Probabilistic Testing
```python
# Randomly sample combinations with weighted probability
@pytest.mark.parametrize("combination",
    random.choices(
        ALL_COMBINATIONS,
        weights=RISK_WEIGHTS,
        k=50  # Run 50 random combinations each time
    )
)
def test_random_sample(combination):
    pass
```

---

# 📊 Practical Implementation

## Reduced Core Matrix
Instead of 14,400 tests, we focus on **strategic coverage**:

```python
CORE_MATRIX = {
    # 4 types × 3 key operators × 2 storage patterns = 24 tests
    "essential_coverage": [
        ("Network", ["eq", "isPrivate", "inSubnet"], ["jsonb_flat", "direct_column"]),
        ("LTree", ["eq", "ancestor_of", "matches_lquery"], ["jsonb_flat", "direct_column"]),
        ("DateRange", ["eq", "contains_date", "overlaps"], ["jsonb_flat", "direct_column"]),
        ("MacAddress", ["eq", "in"], ["jsonb_flat", "direct_column"]),
    ],

    # Plus targeted regression tests for known issues
    "known_failures": [...],  # ~20 tests

    # Plus edge cases for each type
    "edge_cases": [...],      # ~30 tests
}

# Total core suite: ~75 tests instead of 14,400
```

## Environmental Strategy
```python
# Don't test every PostgreSQL version every time
POSTGRES_VERSIONS = {
    "core": ["15"],           # Latest stable only
    "regression": ["12", "17"], # Min/max versions
    "comprehensive": ["12", "13", "14", "15", "16", "17"]  # All versions
}
```

---

# 🚦 Quality Gates

## Development Phase Gates

1. **Local Development**: Core tests only (30s)
2. **Pull Request**: Core + Regression (5min)
3. **Pre-merge**: Core + Regression + Sample Comprehensive (15min)
4. **Pre-release**: Everything except Stress (2hr)
5. **Manual Validation**: Nuclear option (hours)

## Failure Response
```python
# If core tests fail: STOP - Fix immediately
# If regression tests fail: Investigate - May proceed with caution
# If comprehensive tests fail: Review - Block release if critical
# If stress tests fail: Document - Fix in next iteration
```

---

# 💡 Benefits of This Approach

✅ **Fast feedback loop** for developers (30s vs hours)
✅ **Comprehensive coverage** when it matters (releases)
✅ **Intelligent test selection** based on risk and history
✅ **Scalable strategy** that grows with the codebase
✅ **Clear execution tiers** for different scenarios
✅ **Practical for CI/CD** with reasonable resource usage

**Bottom Line**: We get the confidence of comprehensive testing without the pain of running 14,400 tests on every commit.

The **bulletproof plan stays bulletproof**, but becomes **practically executable**.
