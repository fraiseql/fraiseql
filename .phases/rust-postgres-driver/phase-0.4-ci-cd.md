# Phase 0.4: Pre-commit Hooks & CI/CD Pipeline

**Phase**: 0.4 of 0.5 (Part of Phase 0 - Setup)
**Effort**: 1 hour
**Status**: Ready to implement
**Prerequisite**: Phase 0.1-0.3

---

## Objective

Automate code quality checks at commit-time and CI/CD pipeline:

1. Configure prek (Rust pre-commit replacement)
2. Setup GitHub Actions workflows
3. Configure branch protection rules
4. Create PR quality gates

**Success Criteria**:

- ✅ Pre-commit hooks working (clippy, fmt, tests)
- ✅ GitHub Actions workflows passing
- ✅ Branch protection enforced on dev/main
- ✅ PR status checks required before merge

---

## Implementation Steps

### Step 1: Install prek

```bash
# macOS
brew install j178/tap/prek

# Linux
cargo install prek

# Verify
prek --version
```

---

### Step 2: Configure Pre-commit Hooks

**File**: `.pre-commit-config.yaml`

```yaml
repos:
  # Rust formatting
  - repo: local
    hooks:
      - id: rustfmt
        name: rustfmt
        description: Format Rust code
        entry: cargo fmt --all
        language: system
        files: \.rs$
        pass_filenames: false
        stages: [commit]

      - id: clippy
        name: clippy
        description: Rust linting
        entry: cargo clippy --all-targets -- -D warnings
        language: system
        files: \.rs$
        pass_filenames: false
        stages: [commit]

      # File checks
      - id: trailing-whitespace
        name: Trim trailing whitespace
        entry: trailing-whitespace-fixer
        language: system
        types: [text]

      - id: end-of-file-fixer
        name: Fix end of file
        entry: end-of-file-fixer
        language: system
        types: [text]

      - id: check-json
        name: Check JSON
        entry: check-json
        language: system
        types: [json]

      - id: check-yaml
        name: Check YAML
        entry: check-yaml
        language: system
        types: [yaml]

      - id: check-toml
        name: Check TOML
        entry: check-toml
        language: system
        types: [toml]

      # Prevent large files
      - id: check-added-large-files
        name: Check for large files
        entry: check-added-large-files
        language: system
        args: ['--maxkb=1000']
```

---

### Step 3: Setup Pre-commit

```bash
# Install hooks
prek install

# Verify
prek list

# Run on all files
prek run --all

# Run on staged files (happens automatically at commit)
prek run
```

---

### Step 4: GitHub Actions Main Workflow

**File**: `.github/workflows/ci.yml`

```yaml
name: CI Pipeline

on:
  push:
    branches: [ dev, main, staging ]
  pull_request:
    branches: [ dev, main, staging ]

jobs:
  test:
    name: Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-targets -- -D warnings

  fmt:
    name: Formatting
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  coverage:
    name: Code Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install cargo-tarpaulin
      - run: cargo tarpaulin --out Xml --minimum 80
      - uses: codecov/codecov-action@v3
        with:
          files: ./cobertura.xml

  security:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check-action@v1
```

---

### Step 5: Protection Rules

**File**: `.github/settings.yml`

```yaml
branches:
  - name: dev
    protection:
      required_status_checks:
        strict: true
        contexts:
          - "Tests"
          - "Clippy"
          - "Formatting"
          - "Code Coverage"
      required_pull_request_reviews:
        dismiss_stale_reviews: true
        require_code_owner_reviews: true
        required_approving_review_count: 1

  - name: main
    protection:
      required_status_checks:
        strict: true
        contexts:
          - "Tests"
          - "Clippy"
          - "Formatting"
          - "Code Coverage"
          - "Security Audit"
      required_pull_request_reviews:
        dismiss_stale_reviews: true
        require_code_owner_reviews: true
        required_approving_review_count: 2
      enforce_admins: true
```

---

## Success Criteria

- ✅ `prek run --all` succeeds
- ✅ GitHub Actions workflows pass
- ✅ PR cannot merge without all checks passing
- ✅ Dev branch protected
- ✅ Main branch protected

---

**Last Updated**: 2025-12-18
