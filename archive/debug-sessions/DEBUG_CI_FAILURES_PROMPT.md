# Debug CI Failures - Agent Prompt

**Date**: 2025-10-22
**PR**: https://github.com/fraiseql/fraiseql/pull/91
**Branch**: release/v1.0.0-prep

---

## 🎯 Your Mission

The pull request for v1.0 release has **CI failures** even though tests pass locally. Your job is to **investigate and fix** the failing CI checks.

---

## 📊 Current Status

### What Passed Locally ✅
```bash
$ uv run pytest --tb=short
===== 3,551 passed, 10 warnings in 71.86s =====
```

### What Failed in CI ❌
Three checks failed:
1. **Lint** - Failed
2. **Tests** - Failed
3. **Quality Gate** - Failed

**CI Run**: https://github.com/fraiseql/fraiseql/actions/runs/18723951737

---

## 🔍 Step 1: Check CI Test Failures

### What to Do
```bash
# View the failed test logs from CI
gh run view 18723951737 --log-failed

# Or view specific job logs
gh run view 18723951737 --log --job=53403635683  # Tests job
```

### What to Look For
- Which specific tests failed in CI?
- Are they different from local test results?
- Do error messages show environment differences?

### Common CI vs Local Differences
- **PostgreSQL version** - CI might use different PG version
- **Timezone** - CI runs in UTC, local might be different
- **Environment variables** - Missing in CI
- **Dependencies** - Different versions installed
- **File permissions** - CI has stricter permissions
- **Temporary directories** - Different paths in CI

### Questions to Answer
1. **Which tests failed?** (Get the exact test names)
2. **What are the error messages?** (Copy the full traceback)
3. **Are these tests environment-specific?** (Database, auth, caching?)

---

## 🔍 Step 2: Check Lint Failures

### What to Do
```bash
# View lint job logs
gh run view 18723951737 --log --job=53403635700  # Lint job
```

### What to Look For
- Which files have linting errors?
- What are the specific violations?
- Are these ruff/pyright/pre-commit errors?

### Common Lint Issues
- **Line length** (E501) - Lines > 100 characters
- **Import errors** (F401) - Unused imports
- **Type errors** - Missing type hints
- **Formatting** - Black/ruff format issues

### How to Fix Locally
```bash
# Run the same linters as CI
uv run ruff check src/fraiseql/
uv run ruff format src/fraiseql/

# Run pre-commit hooks
pre-commit run --all-files

# Fix specific file
uv run ruff check --fix src/fraiseql/db.py
```

---

## 🔍 Step 3: Check Quality Gate Failure

### What to Do
```bash
# View quality gate logs
gh run view 18723951737 --log --job=53403881128  # Quality Gate job
```

### What to Look For
- What checks does the quality gate run?
- Which specific check failed?
- Is it dependent on Lint or Tests passing?

### Typical Quality Gate Checks
- All tests must pass
- All lints must pass
- Code coverage threshold
- No security vulnerabilities
- Documentation builds successfully

---

## 🛠️ Step 4: Reproduce CI Failures Locally

### Try to Replicate CI Environment

```bash
# 1. Check GitHub Actions workflow file
cat .github/workflows/quality-gate.yml

# 2. Look at what CI runs
#    - Python version
#    - PostgreSQL version
#    - Environment variables
#    - Test commands

# 3. Try to replicate
export CI=true
export GITHUB_ACTIONS=true
# ... other CI env vars

# Run the exact same commands as CI
uv run pytest --tb=short -v
```

### Check for Environment-Specific Tests

```bash
# Look for tests that might behave differently in CI
grep -r "pytest.mark.skip" tests/
grep -r "CI" tests/
grep -r "GITHUB_ACTIONS" tests/
grep -r "localhost" tests/
```

---

## 🐛 Step 5: Common CI Failure Patterns

### Pattern 1: Database Connection Issues

**Symptom**: Tests fail with connection errors in CI

**Cause**: PostgreSQL not running or wrong credentials

**Fix**: Check `.github/workflows/*.yml` for services configuration
```yaml
services:
  postgres:
    image: postgres:15
    env:
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: test_db
```

### Pattern 2: Timing/Race Conditions

**Symptom**: Tests pass locally but fail intermittently in CI

**Cause**: Tests depend on timing, CI might be slower

**Fix**: Add retries or increase timeouts in tests
```python
@pytest.mark.flaky(retries=3)
async def test_timing_sensitive():
    ...
```

### Pattern 3: File Path Issues

**Symptom**: FileNotFoundError in CI

**Cause**: Hardcoded paths or relative path issues

**Fix**: Use `pathlib` or `os.path` for cross-platform paths
```python
from pathlib import Path

BASE_DIR = Path(__file__).parent
config_file = BASE_DIR / "config.yml"
```

### Pattern 4: Missing Dependencies

**Symptom**: Import errors in CI

**Cause**: Dependency not installed or wrong version

**Fix**: Check `pyproject.toml` dependencies match CI environment

### Pattern 5: Pre-commit Hook Failures

**Symptom**: Lint fails with "files were modified by this hook"

**Cause**: Pre-commit modified files but changes weren't committed

**Fix**: Run pre-commit locally, commit changes, push again
```bash
pre-commit run --all-files
git add -A
git commit --amend --no-edit
git push --force-with-lease
```

---

## 📝 Step 6: Document Your Findings

### Create a Report

After investigating, create a file called `CI_FAILURE_ANALYSIS.md`:

```markdown
# CI Failure Analysis - PR #91

## Summary
[Brief description of what failed and why]

## Failed Checks
1. **Tests**: [X tests failed]
   - test_name_1: [Reason]
   - test_name_2: [Reason]

2. **Lint**: [Y files with errors]
   - file_1.py: [Error type]
   - file_2.py: [Error type]

3. **Quality Gate**: [Blocked because...]

## Root Cause
[Detailed explanation]

## Differences: CI vs Local
- Python version: CI=3.13, Local=3.13
- PostgreSQL: CI=15, Local=15
- Environment: CI=Ubuntu, Local=Arch
- [Other differences]

## Fix Applied
[What you did to fix it]

## Verification
```bash
# Commands to verify fix
uv run pytest tests/path/to/test.py
uv run ruff check src/
```

## Lessons Learned
[What we learned from this failure]
```

---

## 🔧 Step 7: Fix and Push

### Workflow

1. **Identify the issue** (Steps 1-5)
2. **Fix locally**
3. **Verify fix works locally**
4. **Commit the fix**
5. **Push to branch**
6. **Wait for CI to run again**
7. **Check if CI passes**

### Example Fix Workflow

```bash
# 1. Fix lint issues
uv run ruff check --fix src/fraiseql/db.py
uv run ruff format src/fraiseql/

# 2. Fix failing tests (if needed)
# Edit test files or source code

# 3. Verify locally
uv run pytest tests/integration/caching/
uv run ruff check src/

# 4. Commit
git add -A
git commit -m "fix: resolve CI failures

- Fixed lint errors in db.py (line length)
- Fixed flaky cache tests with retries
- Updated test environment configuration

Closes #91 CI failures
"

# 5. Push
git push origin release/v1.0.0-prep

# 6. Check CI
gh pr checks 91 --watch
```

---

## 🎯 Success Criteria

You've succeeded when:
- [ ] All 3 CI checks pass (Tests, Lint, Quality Gate)
- [ ] PR shows green checkmarks
- [ ] You understand WHY it failed
- [ ] You documented the root cause
- [ ] You verified the fix works

---

## 🤔 If You Get Stuck

### Try These Commands

```bash
# Compare local and CI environments
python --version
psql --version
uv --version

# Check workflow file
cat .github/workflows/quality-gate.yml

# See what pre-commit runs
cat .pre-commit-config.yaml

# Check test configuration
cat pyproject.toml | grep -A 20 "\[tool.pytest"

# Look for environment-specific code
grep -r "os.getenv\|environ" src/fraiseql/
grep -r "CI\|GITHUB" tests/
```

### Ask These Questions

1. **Is this a real bug or a flaky test?**
   - Run the test multiple times locally
   - Check if it passes sometimes and fails others

2. **Is this environment-specific?**
   - Does it only fail in CI?
   - Does it work on your local machine?

3. **Did recent changes cause this?**
   - Check the git diff: `git diff dev...release/v1.0.0-prep`
   - Look at recent commits: `git log --oneline -10`

4. **Are there similar issues in the past?**
   - Search GitHub issues: https://github.com/fraiseql/fraiseql/issues
   - Check commit history: `git log --grep="CI"`

---

## 📚 Resources

### GitHub CLI Commands
```bash
gh pr checks <pr-number>              # Check status
gh pr checks <pr-number> --watch      # Watch status
gh run list                            # List recent runs
gh run view <run-id>                   # View run details
gh run view <run-id> --log            # View logs
gh run view <run-id> --log-failed     # View only failed logs
```

### Useful Links
- PR: https://github.com/fraiseql/fraiseql/pull/91
- CI Run: https://github.com/fraiseql/fraiseql/actions/runs/18723951737
- Workflow: `.github/workflows/quality-gate.yml`

---

## 💪 You Can Do This!

Remember:
1. **Read the error messages carefully** - They usually tell you exactly what's wrong
2. **Reproduce locally first** - Easier to debug on your machine
3. **Fix one thing at a time** - Don't try to fix everything at once
4. **Test your fix** - Make sure it works before pushing
5. **Document what you learned** - Help future you (and others)

**Good luck! You've got this! 🚀**
