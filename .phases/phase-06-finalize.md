# Phase 6: Finalize

## Objective
Transform working code into production-ready, evergreen repository.

## Steps

### 1. Quality Control Review
Review as a senior software engineer would:
- [ ] API design is intuitive and consistent
- [ ] Error handling is comprehensive
- [ ] Edge cases are covered
- [ ] Performance is acceptable
- [ ] No unnecessary complexity

### 2. Security Audit
Review as a hacker would:
- [ ] Input validation on all boundaries
- [ ] No secrets in code or config
- [ ] Dependencies are minimal and audited
- [ ] No injection vulnerabilities (SQL, command, etc.)
- [ ] Authentication/authorization correct (if applicable)
- [ ] Sensitive data properly handled

### 3. Archaeology Removal
Clean all development artifacts:
- [ ] Remove all `// Phase X:` comments
- [ ] Remove all `# TODO: Phase` markers
- [ ] Remove all `FIXME` without fixing
- [ ] Remove all debugging code
- [ ] Remove all commented-out code
- [ ] Remove `.phases/` directory from main branch
- [ ] Squash/clean git history if appropriate

### 4. Documentation Polish
- [ ] README is accurate and complete
- [ ] API documentation is current
- [ ] No references to development phases
- [ ] Examples work and are tested

### 5. Final Verification
- [ ] All Rust tests pass: `cargo test --all`
- [ ] All Python tests pass: `uv run pytest`
- [ ] Rust lints clean: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Python lints clean: `uv run ruff check`
- [ ] Type checking clean: `uv run ty check`
- [ ] Build succeeds in release mode: `cargo build --release`
- [ ] No TODO/FIXME remaining (except template TODOs in `generate.py` / `init.py`)
- [ ] `git grep -i "phase\|fixme\|hack"` returns nothing unexpected
- [ ] Template TODOs verified as intentional user-facing scaffolding

## Status
[ ] Not Started
