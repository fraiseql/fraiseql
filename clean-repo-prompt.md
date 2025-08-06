# Prompt: Clean Repository with Green GitHub Actions CI/CD

**Goal**: Achieve a fully passing (green) CI/CD pipeline on GitHub Actions for the FraiseQL project.

**Current Status**:

- Security workflow: ✅ Passing
- Performance Benchmarks: ❌ Failing
- Need to ensure all workflows pass consistently

## Tasks to Complete

### 1. Fix Failing Performance Benchmarks

- Debug why the Performance Benchmarks workflow is failing (last run failed after 18m44s)
- Check benchmark scripts in `/benchmarks` directory
- Ensure all database connections and test data are properly set up
- Verify Docker containers start correctly in CI environment

### 2. Ensure All Test Suites Pass

- Run full test suite locally: `pytest tests/ -v`
- Fix any failing tests
- Ensure database tests work with test containers
- Verify all integration tests pass

### 3. Code Quality Checks

- Run and fix all linting issues: `ruff check src/ tests/ --fix`
- Format code: `ruff format src/ tests/`
- Run type checking: `pyright`
- Ensure pre-commit hooks pass: `pre-commit run --all-files`

### 4. Security Scanning

- Ensure no security vulnerabilities in dependencies
- Run security checks locally before pushing
- Verify SAST/dependency scanning passes in CI

### 5. Documentation Build

- Ensure MkDocs builds without errors: `mkdocs build --strict`
- Fix any broken links or missing documentation

### 6. Clean Up Repository

- Remove unnecessary files (logs, cache, temporary files)
- Ensure `.gitignore` is comprehensive
- Remove any accidentally committed sensitive data
- Clean up old/unused Docker images and volumes

### 7. Optimize CI/CD Pipeline

- Review all workflow files in `.github/workflows/`
- Ensure workflows run efficiently
- Add proper caching for dependencies
- Set up matrix testing if needed

### 8. Final Verification

- Push changes to a feature branch
- Create a pull request to trigger all checks
- Ensure ALL status checks pass (green checkmarks)
- Merge only when everything is green

## Success Criteria

- All GitHub Actions workflows show green checkmarks
- No failing tests
- No linting or formatting issues
- No security vulnerabilities
- Documentation builds successfully
- Performance benchmarks complete successfully

## Additional Requirements

- Maintain Python 3.11 compatibility
- Ensure Podman compatibility for container tests
- Keep test coverage above 80%
- All commits should be signed (if configured)

## Commands Reference

```bash
# Run tests
pytest tests/ -v

# Code quality
ruff check src/ tests/ --fix
ruff format src/ tests/
pyright

# Pre-commit
pre-commit run --all-files

# Documentation
mkdocs build --strict

# Check GitHub Actions status
gh run list --limit 10
gh run view <run-id>

# Clean up
git clean -fdx  # Remove untracked files (use with caution)
docker system prune -a  # Clean Docker resources
```
