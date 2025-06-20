# PyPI Publication Quick Reference

## 🚀 Quick Start (First Time)

### 1. Setup PyPI Account
```bash
# Create account at https://pypi.org
# Enable 2FA (recommended)
# Generate API token: Account Settings → API tokens → Add API token
```

### 2. Configure Authentication
```bash
# Create ~/.pypirc
cat > ~/.pypirc << EOF
[pypi]
username = __token__
password = pypi-YOUR-TOKEN-HERE
EOF

chmod 600 ~/.pypirc
```

### 3. Test with TestPyPI First
```bash
make publish-test
# Follow prompts, test installation:
pip install -i https://test.pypi.org/simple/ fraiseql
```

### 4. Publish to PyPI
```bash
make publish
# Confirm when prompted
```

## 📋 Pre-flight Checklist

```bash
# 1. Run all checks
make qa

# 2. Update version (if needed)
# Edit pyproject.toml or create git tag

# 3. Update CHANGELOG.md
# Document all changes

# 4. Build and check
make build
make check-publish
```

## 🛠️ Manual Commands

```bash
# Clean build artifacts
rm -rf dist/ build/ *.egg-info

# Build package
python -m build

# Check package
python -m twine check dist/*

# Upload to TestPyPI
python -m twine upload --repository testpypi dist/*

# Upload to PyPI
python -m twine upload dist/*
```

## 🔧 Makefile Commands

| Command | Description |
|---------|-------------|
| `make build` | Build distribution packages |
| `make check-publish` | Validate packages with twine |
| `make publish-test` | Upload to TestPyPI |
| `make publish` | Upload to PyPI (production) |
| `make qa` | Run all quality checks |

## 📦 Version Management

```bash
# Check current version
python -m setuptools_scm

# Create release tag
git tag -a v0.1.0 -m "Release version 0.1.0"
git push origin v0.1.0
```

## 🚨 Common Issues

### "Invalid distribution file"
```bash
# Rebuild from clean state
make clean
make build
```

### "Version already exists"
- You cannot overwrite PyPI versions
- Bump version number and rebuild

### "Authentication failed"
- Check ~/.pypirc has correct token
- Ensure token has upload permissions

## 📝 Post-Release

1. **Create GitHub Release**
   - Go to Releases → Create new release
   - Choose tag, add changelog
   - Upload dist/* files as assets

2. **Verify Installation**
   ```bash
   pip install fraiseql
   python -c "import fraiseql; print(fraiseql.__version__)"
   ```

3. **Update Documentation**
   - Add PyPI badges to README
   - Update installation instructions

## 🔗 Important URLs

- **PyPI Project**: https://pypi.org/project/fraiseql/
- **TestPyPI Project**: https://test.pypi.org/project/fraiseql/
- **Account Settings**: https://pypi.org/manage/account/
- **Token Help**: https://pypi.org/help/#apitoken

## ⚡ One-Liner Release (Experienced Users)

```bash
# After all checks pass and version is tagged:
make qa && make build && make publish
```

---

**Remember**: Always test with TestPyPI first! 🧪
