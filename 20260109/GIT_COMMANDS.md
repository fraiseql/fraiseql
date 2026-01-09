# Git Commands Reference

## Starting Tomorrow's Work

```bash
# Verify current state
cd /home/lionel/code/fraiseql
git status
git log --oneline -1

# Should show:
# On branch feature/phase-16-rust-http-server
# 0cdae0c6 feat(phase-3.2): Query execution foundation - corrected architecture
```

## During Development

### Building and Testing

```bash
# Quick type check (faster)
cargo check --lib

# Full build
cargo build --lib

# Build everything (strict)
cargo build --lib --all-targets

# Run tests (watch for failures)
python -m pytest tests/ -q --tb=short

# Run specific test
python -m pytest tests/db/ -q --tb=short
```

### Code Changes

```bash
# Check what changed
git status

# Show changes to specific file
git diff fraiseql_rs/src/db/pool_production.rs

# Show all changes
git diff

# Add specific file
git add fraiseql_rs/src/db/pool_production.rs

# Add all changes
git add -A
```

### Formatting Code

```bash
# Apply cargo fix suggestions
cargo fix --lib -p fraiseql

# Format with rustfmt
cargo fix --lib -p fraiseql --allow-dirty

# Check formatting (no changes)
cargo fmt --check
```

## Committing Work

### When Task 4 (Query Execution) is Complete

```bash
# Stage changes
git add -A

# Verify what will be committed
git status

# Show diff before committing
git diff --cached

# Commit with message
git commit -m "feat(phase-3.2): Implement query execution in ProductionPool

- Implement query() method for SELECT execution
- Extract JSONB from PostgreSQL column 0
- Return results as Vec<serde_json::Value>
- Add parameter validation integration
- Add unit tests for query execution
- Add integration tests with real PostgreSQL
- All 7467 tests passing, no regressions"

# Verify commit was created
git log --oneline -1
```

### If You Need to Amend Last Commit

```bash
# Make changes to files
git add .

# Add to previous commit
git commit --amend --no-edit

# Or with new message
git commit --amend -m "new message"
```

## Checking History

```bash
# Show last 5 commits
git log --oneline -5

# Show commits for specific file
git log --oneline fraiseql_rs/src/db/pool_production.rs

# Show full commit details
git show 0cdae0c6

# Show what changed in commit
git show 0cdae0c6 --stat

# Compare with main branch
git log --oneline origin/dev..HEAD
```

## Branches

```bash
# Current branch
git branch

# List all branches
git branch -a

# Show branch upstream
git status

# Switch branch
git checkout dev
git checkout feature/phase-16-rust-http-server

# Create new branch
git checkout -b fix/issue-name
```

## Dealing with Errors

### If Pre-commit Hooks Block Commit

```bash
# Option 1: Fix the issues (preferred)
# Follow the error messages to fix problems

# Option 2: Bypass hooks (for now only)
git commit --no-verify -m "message"

# Option 3: Fix code and try again
cargo fix --lib -p fraiseql
cargo fmt
git add -A
git commit -m "message"
```

### If You Made a Mistake

```bash
# Undo last commit (keep changes)
git reset --soft HEAD~1

# Undo last commit (discard changes)
git reset --hard HEAD~1

# Undo file changes
git restore fraiseql_rs/src/db/pool_production.rs

# Undo staged changes
git restore --staged fraiseql_rs/src/db/pool_production.rs
```

### If You Need to Clean Up

```bash
# Show untracked files
git status

# Remove untracked files (be careful!)
git clean -fd

# Remove untracked files that git ignores
git clean -fdX

# Dry run (show what would be deleted)
git clean -fdn
```

## Pushing Changes

### After Phase 3.2 Tasks Complete

```bash
# Push to current branch
git push

# Or explicit
git push origin feature/phase-16-rust-http-server

# Push with tags
git push --tags

# Force push (only if necessary, use with caution)
git push --force-with-lease
```

### Before Merging to Dev

```bash
# Fetch latest
git fetch origin

# Rebase on dev (if needed)
git rebase origin/dev

# Or merge dev into current branch
git merge origin/dev
```

## Useful Git Aliases

You can add these to `~/.gitconfig` for shortcuts:

```bash
git config --global alias.co checkout
git config --global alias.br branch
git config --global alias.ci commit
git config --global alias.st status
git config --global alias.log1 "log --oneline -1"
git config --global alias.logg "log --oneline --graph --all"
git config --global alias.diff-cached "diff --cached"
```

Then use:
```bash
git co feature/phase-16-rust-http-server
git st
git log1
git ci -m "message"
```

## Working with Remote

```bash
# Show remote
git remote -v

# Fetch all changes from remote (no merge)
git fetch

# Pull changes (fetch + merge)
git pull

# Pull with rebase
git pull --rebase

# Push current branch
git push -u origin feature/phase-16-rust-http-server
```

## Stashing Changes (If Interrupted)

```bash
# Save current work without committing
git stash

# List stashed changes
git stash list

# Restore stashed changes
git stash pop

# Apply without removing from stash
git stash apply

# Discard stashed changes
git stash drop
```

## Daily Workflow Summary

```bash
# Morning: Start work
cd /home/lionel/code/fraiseql
git status
git pull (to sync with team if needed)

# During work: Build and test frequently
cargo build --lib
python -m pytest tests/ -q

# When done: Commit work
git add -A
git status  # verify changes
git commit -m "descriptive message"
git log --oneline -1  # verify commit

# Evening: Push if complete
git push
```

## Emergency Commands

```bash
# Find a lost commit (shows recent actions)
git reflog

# Recover a lost commit
git reset --hard <commit-hash>

# Check for conflicts in merge
git status  # will show conflicted files

# Abort a failed merge
git merge --abort

# Abort a failed rebase
git rebase --abort
```

## Reviewing Before Commit

```bash
# See exactly what you changed
git diff

# See only file names
git diff --name-only

# See staged changes
git diff --cached

# See word-by-word changes
git diff --word-diff

# Compare current vs main branch
git diff origin/dev..HEAD
```

## Creating a Commit Message Template

```bash
# Current task commit message
git commit -m "feat(phase-3.2): Implement query execution in ProductionPool

- Brief description of what you implemented
- List of key changes
- Any notes for reviewers
- Reference any related issues or PRs"
```

---

## Quick Reference

| Command | Purpose |
|---------|---------|
| `git status` | Check current state |
| `git add -A` | Stage all changes |
| `git commit -m "msg"` | Create commit |
| `git push` | Push to remote |
| `git log -1` | Show last commit |
| `git diff` | Show changes |
| `git reset --soft HEAD~1` | Undo commit (keep changes) |
| `git restore <file>` | Discard file changes |
| `git stash` | Temporarily save work |

---

**Remember**: Always check `git status` before committing!
