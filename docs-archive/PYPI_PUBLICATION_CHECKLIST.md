# PyPI Publication Checklist for FraiseQL

This checklist ensures a smooth and professional release to PyPI (Python Package Index).

## Pre-Release Checklist

### 1. Code Quality ✓
- [ ] All tests pass: `make test`
- [ ] Linting passes: `make lint`
- [ ] Type checking passes: `make type-check`
- [ ] Code formatting: `make format`
- [ ] No security vulnerabilities: `pip audit`

### 2. Version Management
- [ ] Update version in `pyproject.toml` (if not using dynamic versioning)
- [ ] Create git tag: `git tag -a v0.1.0 -m "Release version 0.1.0"`
- [ ] Update `CHANGELOG.md` with release notes
- [ ] Verify version: `python -m setuptools_scm`

### 3. Documentation
- [ ] README.md is up-to-date and renders correctly
- [ ] Documentation builds without errors: `mkdocs build`
- [ ] API documentation is complete
- [ ] Installation instructions are clear
- [ ] Quick start guide works for new users
- [ ] All links in documentation work

### 4. Package Metadata (`pyproject.toml`)
- [ ] `name` - Correct package name (fraiseql)
- [ ] `description` - Clear, concise description
- [ ] `readme` - Points to README.md
- [ ] `license` - License specified (MIT)
- [ ] `authors` - Author information complete
- [ ] `keywords` - Relevant keywords for discovery
- [ ] `classifiers` - Appropriate PyPI classifiers
  ```toml
  classifiers = [
    "Development Status :: 4 - Beta",
    "Intended Audience :: Developers",
    "Topic :: Software Development :: Libraries :: Python Modules",
    "License :: OSI Approved :: MIT License",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.13",
    "Framework :: FastAPI",
    "Topic :: Database",
    "Topic :: Internet :: WWW/HTTP :: HTTP Servers",
  ]
  ```
- [ ] `urls` - All project URLs working
- [ ] `dependencies` - All runtime dependencies listed
- [ ] `optional-dependencies` - Optional deps organized

### 5. Package Structure
- [ ] `src/fraiseql/__init__.py` exports public API
- [ ] No test files included in package
- [ ] No development files included (.env, .gitignore, etc.)
- [ ] `py.typed` file exists for type hints
- [ ] All necessary data files included

### 6. Build Configuration
- [ ] Build backend specified: `build-backend = "setuptools.build_meta"`
- [ ] Package discovery configured correctly
- [ ] Excluded unnecessary files in build

## Build and Test Checklist

### 7. Local Build Test
```bash
# Clean previous builds
rm -rf dist/ build/ *.egg-info

# Build the package
python -m build

# Check the built files
ls -la dist/
```

- [ ] Both `.whl` and `.tar.gz` files created
- [ ] File sizes are reasonable (not too large)

### 8. Package Content Verification
```bash
# Check wheel contents
unzip -l dist/fraiseql-*.whl

# Check sdist contents
tar -tzf dist/fraiseql-*.tar.gz
```

- [ ] Only necessary files included
- [ ] No sensitive files (secrets, .env, etc.)
- [ ] Package structure looks correct

### 9. Test Installation
```bash
# Create a fresh virtual environment
python -m venv test_env
source test_env/bin/activate

# Install from built wheel
pip install dist/fraiseql-*.whl

# Test import
python -c "import fraiseql; print(fraiseql.__version__)"

# Test CLI if applicable
fraiseql --version

# Clean up
deactivate
rm -rf test_env
```

- [ ] Package installs without errors
- [ ] All dependencies resolved correctly
- [ ] Import works
- [ ] CLI commands work (if applicable)

### 10. TestPyPI Upload (Recommended First)
```bash
# Upload to TestPyPI
python -m twine upload --repository testpypi dist/*

# Test installation from TestPyPI
pip install --index-url https://test.pypi.org/simple/ --extra-index-url https://pypi.org/simple fraiseql
```

- [ ] Upload successful
- [ ] Package page looks correct on TestPyPI
- [ ] Installation from TestPyPI works

## PyPI Release Checklist

### 11. PyPI Account Setup
- [ ] PyPI account created and verified
- [ ] 2FA enabled on PyPI account (recommended)
- [ ] API token generated (not password)
- [ ] API token saved securely

### 12. Configure Authentication
Create `~/.pypirc`:
```ini
[distutils]
index-servers =
    pypi
    testpypi

[pypi]
repository = https://upload.pypi.org/legacy/
username = __token__
password = pypi-YOUR-API-TOKEN-HERE

[testpypi]
repository = https://test.pypi.org/legacy/
username = __token__
password = pypi-YOUR-TEST-API-TOKEN-HERE
```

- [ ] `.pypirc` configured with API tokens
- [ ] File permissions set: `chmod 600 ~/.pypirc`

### 13. Final Upload to PyPI
```bash
# Final check
python -m twine check dist/*

# Upload to PyPI
python -m twine upload dist/*
```

- [ ] All checks pass
- [ ] Upload successful
- [ ] Package visible on https://pypi.org/project/fraiseql/

### 14. Post-Release Verification
- [ ] Install from PyPI: `pip install fraiseql`
- [ ] Verify correct version installed
- [ ] Test basic functionality
- [ ] Check package page on PyPI
- [ ] Documentation links work
- [ ] Download statistics updating

### 15. GitHub Release
- [ ] Create GitHub release from tag
- [ ] Add release notes from CHANGELOG
- [ ] Attach built wheel and sdist files
- [ ] Mark as pre-release if applicable

### 16. Announcements
- [ ] Update project README with PyPI badge
- [ ] Post on relevant forums/communities
- [ ] Update project website (if applicable)
- [ ] Notify users via mailing list (if applicable)

## Automation Setup (For Future Releases)

### 17. GitHub Actions for PyPI (Optional)
Create `.github/workflows/publish.yml`:
```yaml
name: Publish to PyPI

on:
  release:
    types: [published]

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
    - name: Set up Python
      uses: actions/setup-python@v5
      with:
        python-version: '3.13'
    - name: Install dependencies
      run: |
        pip install build twine
    - name: Build package
      run: python -m build
    - name: Publish to PyPI
      env:
        TWINE_USERNAME: __token__
        TWINE_PASSWORD: ${{ secrets.PYPI_API_TOKEN }}
      run: twine upload dist/*
```

- [ ] GitHub secret `PYPI_API_TOKEN` added
- [ ] Workflow tested with TestPyPI first
- [ ] Workflow triggers on release

## Common Issues and Solutions

### Package Name Conflicts
- Check if name is available: https://pypi.org/project/fraiseql/
- Have backup names ready

### README Rendering Issues
- Validate README: `python -m readme_renderer README.md`
- Use `twine check` before upload

### Missing Files in Package
- Check `MANIFEST.in` if needed
- Verify `package_data` in setup configuration

### Version Conflicts
- Ensure version is higher than any existing release
- Use semantic versioning (MAJOR.MINOR.PATCH)

## Emergency Rollback

If something goes wrong:
1. **Cannot delete from PyPI** - versions are immutable
2. **Can yank a release** - marks it as broken
3. **Upload a new fixed version** with bumped version number

```bash
# Yank a broken release (marks it as unsafe)
# Must be done via PyPI web interface
```

## Success Criteria

- [ ] Users can `pip install fraiseql` successfully
- [ ] Package works as expected after installation
- [ ] Documentation is accessible and helpful
- [ ] No critical bugs reported in first 24 hours

---

**Remember**: Once published to PyPI, you cannot delete or overwrite a version. Always test thoroughly with TestPyPI first!
