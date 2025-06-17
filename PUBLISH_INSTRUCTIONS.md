# Publishing FraiseQL 0.1.0a7 to PyPI

The release is ready to publish. The package has been built and tagged.

## What's Ready

1. **Version bumped to 0.1.0a7** in:
   - `pyproject.toml`
   - `src/fraiseql/__init__.py`

2. **Git tag created**: `v0.1.0a7`

3. **Distribution files built**:
   - `dist/fraiseql-0.1.0a7-py3-none-any.whl`
   - `dist/fraiseql-0.1.0a7.tar.gz`

4. **Release notes**: `RELEASE_NOTES_0.1.0a7.md`

## To Publish

Run one of these commands with PyPI credentials:

```bash
# Using uv (requires PyPI token)
uv publish

# Or using twine
pip install twine
twine upload dist/fraiseql-0.1.0a7*
```

## What's New in 0.1.0a7

- **N+1 Query Detection**: Automatic detection in development mode
- **Strawberry Migration**: Complete toolkit with compatibility layer
- **JSON Type Support**: Full dict[str, Any] and fraiseql.JSON support

## After Publishing

1. Push commits and tags to GitHub:
   ```bash
   git push origin main
   git push origin v0.1.0a7
   ```

2. Create GitHub release from the tag

3. Verify on PyPI: https://pypi.org/project/fraiseql/0.1.0a7/