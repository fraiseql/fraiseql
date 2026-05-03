# Phase 5: Finalize

## Objective

Clean the branch history and merge to dev once all verification phases pass.

## Steps

### 1. Quality Control Review

- [ ] All phase tests pass locally
- [ ] `cargo clippy --workspace --all-targets --exclude fraiseql-storage -- -D warnings` clean
- [ ] `cargo test --workspace --exclude fraiseql-storage` — only Docker-dependent tests skipped
- [ ] No TODO/FIXME left in changed files
- [ ] No debug prints or commented-out code

### 2. Security Audit of Implementation

- [ ] Schema hash cannot be bypassed by reordering JSON fields
- [ ] Cache bypass cannot be toggled by user-controlled input
- [ ] Tenant cross-validation cannot be disabled by omitting headers
- [ ] Rate limit tenant key cannot be spoofed
- [ ] mTLS key material not logged or leaked in error messages

### 3. Branch Cleanup

- [ ] Interactive rebase: squash fix commits into their respective feature commits
- [ ] Target: ~7 clean commits (one per remediation item + the cleanup)
- [ ] Each commit message has clear `## Changes` and `## Verification` sections
- [ ] Force-push cleaned branch (with user confirmation)

### 4. Merge

- [ ] Create PR against `dev`
- [ ] PR description references the original audit findings
- [ ] Merge (squash or rebase per user preference)
- [ ] Delete feature branch
- [ ] Remove `.phases/` directory from dev (or keep if user prefers)

## Dependencies

- Phases 1–4 complete

## Status
[ ] Not Started
