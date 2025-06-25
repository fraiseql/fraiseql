# Release Checklist for v0.1.0a13

## Pre-Release Checklist

✅ **Code Changes**
- [x] Implemented dual-mode repository instantiation feature
- [x] Added `find()` and `find_one()` methods to FraiseQLRepository
- [x] Added mode detection from environment and context
- [x] Implemented recursive object instantiation with circular reference handling
- [x] Added UUID and datetime type conversion
- [x] Added camelCase to snake_case conversion
- [x] Maximum recursion depth protection

✅ **Testing**
- [x] Created comprehensive unit tests (11 tests)
- [x] All tests passing
- [x] Removed all domain-specific (PrintOptim) references from tests
- [x] Tests use generic e-commerce examples (Product, Order, etc.)

✅ **Documentation**
- [x] Updated CHANGELOG.md with v0.1.0a13 entry
- [x] Created detailed RELEASE_NOTES_v0.1.0a13.md
- [x] Added inline documentation to new methods

✅ **Version Updates**
- [x] Updated version in pyproject.toml to 0.1.0a13
- [x] Updated version in src/fraiseql/__init__.py to 0.1.0a13

✅ **Code Quality**
- [x] Ran linting (ruff) - minor warnings in SQL strings are acceptable
- [x] Code formatted with ruff format
- [x] Added noqa comments where needed

## Files Changed

### Core Implementation
- `src/fraiseql/db.py` - Added dual-mode functionality

### Tests
- `tests/test_dual_mode_repository.py` - Integration tests (requires DB)
- `tests/test_dual_mode_repository_unit.py` - Unit tests (no DB required)

### Documentation
- `CHANGELOG.md` - Added v0.1.0a13 entry
- `RELEASE_NOTES_v0.1.0a13.md` - Detailed release notes

### Version Files
- `pyproject.toml` - Version bump
- `src/fraiseql/__init__.py` - Version bump

## Release Steps

1. **Review Changes**
   ```bash
   git status
   git diff
   ```

2. **Run Full Test Suite** (if database available)
   ```bash
   pytest
   ```

3. **Build Package**
   ```bash
   python -m build
   ```

4. **Create Git Tag**
   ```bash
   git add .
   git commit -m "feat: Add dual-mode repository instantiation for dev/prod environments"
   git tag v0.1.0a13
   ```

5. **Push Changes**
   ```bash
   git push origin main
   git push origin v0.1.0a13
   ```

6. **Create GitHub Release**
   - Use the content from RELEASE_NOTES_v0.1.0a13.md
   - Attach the built wheel and sdist

7. **Publish to PyPI**
   ```bash
   twine upload dist/fraiseql-0.1.0a13*
   ```

## Post-Release

- [ ] Verify package on PyPI
- [ ] Test installation: `pip install fraiseql==0.1.0a13`
- [ ] Update any documentation sites
- [ ] Announce release (if applicable)