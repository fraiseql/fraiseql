# FraiseQL Release Cleanup Plan

## Overview
Prepare FraiseQL codebase for next production release by addressing code quality issues and ensuring all components meet production standards.

## 1. Code Quality Fixes

### Fix Ruff Linting Issues
- **Line Length (E501)**: Refactor long error messages in scalar types
  - Files: `cusip.py`, `isin.py`, `stock_symbol.py`, `exchange_rate.py`
  - Break long strings across multiple lines or shorten messages

- **Type Annotations (ANN201)**: Add return type annotations to all test methods
  - Add `-> None` to all test methods across `tests/unit/core/type_system/`
  - ~780+ missing annotations to add

- **Exception Types (TRY004)**: Use `TypeError` for type validation
  - Files: `exchange_rate.py`, `money.py`, `percentage.py`
  - Change `ValueError` to `TypeError` where checking type validity

- **Docstring Format (D301)**: Fix raw string docstrings
  - File: `markdown.py`
  - Add `r` prefix to docstrings containing backslashes

### Run Full Quality Check
```bash
uv run ruff check --fix src/ tests/
uv run ruff format src/ tests/
uv run mypy src/
```

## 2. Test Suite Verification

### Run Complete Test Suite
```bash
uv run pytest --tb=short -v
uv run pytest --cov=src --cov-report=term-missing
```

### Fix Any Failing Tests
- Ensure all 49+ scalar types have passing tests
- Verify integration tests pass
- Check edge cases and error handling

## 3. Documentation Updates

### Update Main README
- Add newly implemented scalar types to feature list
- Update scalar count (49+ types)
- Add usage examples for new scalars

### Update CHANGELOG
- Document all new scalars added in Phase 5
- List breaking changes (if any)
- Add migration guide if needed

### Verify Inline Documentation
- Check all scalars have proper docstrings
- Ensure examples in docstrings are accurate
- Verify GraphQL descriptions are present

## 4. Dependency Management

### Review Dependencies
```bash
uv pip list --outdated
```

- Update dependencies if needed
- Ensure `validators` library is properly documented
- Check for security vulnerabilities

### Lock File Verification
```bash
uv lock --check
```

## 5. Performance & Security

### Security Scan
- Review all regex patterns for ReDoS vulnerabilities
- Check input validation in all scalars
- Ensure no SQL injection risks in generated queries

### Performance Check
- Profile scalar validation performance
- Ensure regex patterns are optimized
- Check for any O(n²) operations

## 6. Git Hygiene

### Branch Cleanup
- Merge `autofraiseql` to `dev` branch
- Delete stale feature branches
- Tag release version

### Pre-Release Checklist
- [ ] All tests passing
- [ ] All linting issues resolved
- [ ] Documentation updated
- [ ] CHANGELOG complete
- [ ] Version bumped in `pyproject.toml`
- [ ] No debug code or TODOs in production code

## 7. Build & Distribution

### Verify Build
```bash
uv build
```

### Test Installation
```bash
uv pip install dist/*.whl
# Test import and basic functionality
```

## Estimated Effort
- Code quality fixes: 4-6 hours
- Testing & verification: 2-3 hours
- Documentation: 2-3 hours
- Final review & release: 1-2 hours

**Total: ~10-15 hours**

## Success Criteria
- ✅ Zero linting errors
- ✅ 100% test pass rate
- ✅ All new features documented
- ✅ Clean git history
- ✅ Production-ready build
