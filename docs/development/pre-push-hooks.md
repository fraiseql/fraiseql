# Pre-Push Hooks - Prevent Pushing Broken Code

## Overview

FraiseQL uses pre-commit hooks to prevent pushing broken code to the remote repository. Tests must pass locally before you can push.

## What Happens When You Push

When you run `git push`, the pre-push hook automatically:

1. **Checks your environment**: Verifies `uv` is installed
2. **Runs tests**: Executes the test suite (excluding slow integration tests)
3. **Blocks push if tests fail**: Prevents broken code from reaching the remote
4. **Allows push if tests pass**: Pushes your code to remote

## Example Output

### ✅ Tests Pass (Push Allowed)

```bash
$ git push origin dev

🔒 PRE-PUSH PROTECTION: Running tests before push...
📊 This prevents pushing broken code to remote repository

tests/unit/mutations/test_rust_executor.py ....          [ 12%]
tests/integration/graphql/mutations/test_mutation_dict_responses.py ..  [ 18%]
... (more tests)

✅ All tests passed - push allowed
🚀 Pushing to remote repository...

Enumerating objects: 5, done.
Counting objects: 100% (5/5), done.
...
```

### ❌ Tests Fail (Push Blocked)

```bash
$ git push origin dev

🔒 PRE-PUSH PROTECTION: Running tests before push...
📊 This prevents pushing broken code to remote repository

tests/unit/mutations/test_rust_executor.py ..F.          [ 12%]

=========================== FAILURES ===========================
...

❌ TESTS FAILED - PUSH BLOCKED
🚨 Cannot push broken code to remote repository
💡 Fix failing tests and try again
🔧 Run: uv run pytest --tb=short -v

error: failed to push some refs to 'origin'
```

## Installation

Pre-push hooks are automatically installed when you run:

```bash
pre-commit install --hook-type pre-push
```

This is typically done during initial project setup.

## Skipping the Hook (Not Recommended)

**⚠️ Warning**: Skipping pre-push hooks can introduce broken code into the repository.

If you absolutely must skip (e.g., pushing documentation-only changes):

```bash
git push --no-verify
```

**Better approach**: Fix the failing tests instead of skipping the hook.

## What Tests Are Run

The pre-push hook runs:
- ✅ Unit tests
- ✅ Integration tests (excluding slow blog examples)
- ✅ Mutation tests
- ❌ Performance tests (excluded - too slow)
- ❌ Blog example tests (excluded - slow)

**Estimated time**: 30-60 seconds for typical changes

## CI/CD Behavior

The pre-push hook automatically skips in CI environments:
- GitHub Actions
- pre-commit.ci
- Other CI services

Tests run normally in CI/CD pipelines - the hook only runs locally.

## Configuration

Pre-push hook configuration is in `.pre-commit-config.yaml`:

```yaml
- repo: local
  hooks:
    - id: pytest-pre-push
      name: pytest (pre-push - all tests must pass)
      stages: [pre-push]  # Only runs on git push
      # ... test command ...
```

## Troubleshooting

### Hook Not Running

If the hook doesn't run when you push:

```bash
# Reinstall hooks
pre-commit install --hook-type pre-push

# Verify installation
ls -la .git/hooks/pre-push
```

### Tests Taking Too Long

If tests are too slow, you can temporarily disable the hook:

```bash
# Disable
pre-commit uninstall --hook-type pre-push

# Re-enable when done
pre-commit install --hook-type pre-push
```

### Missing uv

If you get "uv not found" error:

```bash
# Install uv
curl -LsSf https://astral.sh/uv/install.sh | sh

# Verify
uv --version
```

## Benefits

✅ **Prevents broken builds**: Catches failures before they reach remote
✅ **Faster feedback**: Know immediately if code breaks tests
✅ **Better git history**: Keeps main/dev branches clean
✅ **Team protection**: Other developers don't pull broken code
✅ **CI/CD efficiency**: Reduces failed CI runs

## Related

- [Pre-Commit Hooks](../../.pre-commit-config.yaml) - All hooks configuration
- [Running Tests](../testing/developer-guide.md) - How to run tests manually
- [CI/CD](../../.github/workflows/) - GitHub Actions configuration
