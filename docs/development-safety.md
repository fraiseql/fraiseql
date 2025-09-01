# Development Safety Guide

## Overview

This guide covers the safety mechanisms in place to prevent broken code from reaching the repository and production.

## 🛡️ Multiple Layers of Protection

### Layer 1: Pre-commit Hooks
**When**: Every `git commit`
**What**: Runs comprehensive test suite locally
**Purpose**: Catch issues before they enter git history

```bash
# Pre-commit hooks automatically run on every commit
git commit -m "your changes"  # Tests run automatically

# To run pre-commit hooks manually
make pre-commit
```

### Layer 2: Pre-push Hooks
**When**: Every `git push`
**What**: Runs full test suite before push to remote
**Purpose**: Final safety check before code reaches remote repository

```bash
# Pre-push hooks automatically run on every push
git push origin branch-name  # Tests run automatically
```

### Layer 3: GitHub Quality Gates
**When**: PR creation, branch pushes
**What**: CI/CD runs comprehensive test suite
**Purpose**: Prevent broken code from reaching main/dev branches

### Layer 4: Branch Protection Rules
**When**: PR merge attempts
**What**: Requires all CI checks to pass
**Purpose**: Enforce quality standards at repository level

## 🚀 Safe Development Workflow

### Recommended Commands

```bash
# 1. Safe commit workflow
make safe-commit              # Runs tests first, then prompts for commit
git add -A && git commit -m "message"

# 2. Safe push workflow
make safe-push               # Runs full tests, then prompts for push
git push origin branch-name

# 3. Quick test verification
make verify-tests            # Check current test status
```

### Manual Safety Checks

```bash
# Run core tests (fast)
make test-core

# Run full test suite (comprehensive)
make test

# Check pre-commit setup
make test-commit-safety
```

## ❌ What Went Wrong Previously

The issue occurred because:

1. **Incomplete local testing** - Individual tests passed but full suite wasn't run
2. **Pre-commit bypass** - Tests may have been skipped due to environment issues
3. **No pre-push verification** - Code was pushed without final verification

## ✅ Current Protections

### Enhanced Pre-commit Hook
- **Strict error handling** (`set -e`)
- **Clear failure messages**
- **Comprehensive test run** with `-x` flag (fail fast)
- **Environment validation** (checks for uv)

### New Pre-push Hook
- **Full test suite** runs before every push
- **Blocks push** if any test fails
- **Clear error reporting**

### Improved Makefile Commands
- **`make safe-commit`** - Test before commit
- **`make safe-push`** - Test before push
- **`make verify-tests`** - Quick status check

## 🔧 Setup Instructions

### 1. Install Pre-commit Hooks
```bash
make pre-commit-install
```

### 2. Verify Setup
```bash
make test-commit-safety
```

### 3. Test the Protection
```bash
# Try committing with a failing test (should be blocked)
echo "assert False" >> tests/test_example.py
git add tests/test_example.py
git commit -m "test"  # Should fail and block commit
git restore tests/test_example.py  # Restore file
```

## 🚨 Emergency Bypass (Use Sparingly)

In rare cases where you need to bypass safety checks:

```bash
# Skip pre-commit hooks (not recommended)
git commit --no-verify -m "emergency fix"

# Skip pre-push hooks (not recommended)
git push --no-verify origin branch-name
```

**⚠️ Warning**: Only use bypass in genuine emergencies and ensure tests pass ASAP.

## 📊 Monitoring and Debugging

### Check Hook Status
```bash
# List installed hooks
ls -la .git/hooks/

# Test pre-commit
pre-commit run --all-files

# Check if hooks are executable
ls -l .git/hooks/pre-*
```

### Debug Test Failures
```bash
# Run tests with verbose output
make test-core -v

# Run specific failing test
pytest tests/path/to/test.py::TestClass::test_method -v

# Check test environment
make verify-tests
```

## 💡 Best Practices

### For Developers
1. **Always run `make test-core` before committing**
2. **Use `make safe-commit` for important changes**
3. **Check `make verify-tests` if unsure about test status**
4. **Never bypass hooks without good reason**

### For Code Reviews
1. **Verify CI passes before reviewing**
2. **Check that tests cover new functionality**
3. **Ensure branch protection rules are enabled**

### For Repository Maintenance
1. **Monitor hook effectiveness**
2. **Update safety mechanisms as needed**
3. **Review bypass usage in git logs**
4. **Keep protection documentation updated**

## 🔗 Related Documentation

- [CI/CD Pipeline Documentation](ci-cd-pipeline.md)
- [Quality Gate Workflow](.github/workflows/quality-gate.yml)
- [Branch Protection Settings](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/defining-the-mergeability-of-pull-requests)

---

**Remember**: These safety mechanisms exist to maintain code quality and prevent production issues. They're not obstacles—they're safeguards! 🛡️
