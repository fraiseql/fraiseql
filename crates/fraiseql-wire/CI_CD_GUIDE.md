# CI/CD Guide for fraiseql-wire

This guide explains the Continuous Integration and Continuous Deployment (CI/CD) setup for fraiseql-wire.

---

## Overview

The fraiseql-wire project uses GitHub Actions for automated testing, quality checks, and releases.

### Workflows

1. **CI Workflow** (`.github/workflows/ci.yml`)
   - Runs on every push to main/develop and pull requests
   - Tests, linting, formatting, security audit, coverage
   - ~5-10 minutes total

2. **Release Workflow** (`.github/workflows/release.yml`)
   - Runs when a git tag is created (v0.2.0, etc.)
   - Builds, tests, creates GitHub Release
   - Publishes to crates.io
   - ~5-10 minutes total

---

## CI Workflow Details

### Jobs

#### 1. Build & Test

```yaml
name: Build
- Installs Rust (stable)
- Caches dependencies
- Builds with `cargo build --release`
- Runs unit tests
- Runs clippy (linting)
- Checks code formatting
- Security audit with `cargo audit`
```

**Status**: Shows passing ✅ or failing ❌

#### 2. Coverage

```yaml
name: Code Coverage
- Installs cargo-tarpaulin
- Generates coverage report
- Uploads to Codecov.io
- Target: > 85% coverage
```

**Badge**: Can be displayed in README

#### 3. MSRV (Minimum Supported Rust Version)

```yaml
name: MSRV (Rust 1.70)
- Tests with Rust 1.70
- Ensures backward compatibility
- Verifies no new 1.71+ features used
```

#### 4. Integration Tests

```yaml
name: Integration Tests
- Starts Postgres 15 service
- Initializes test database
- Runs integration tests (marked #[ignore])
- Tests actual database operations
```

#### 5. Documentation

```yaml
name: Documentation
- Builds rustdoc
- Checks for doc warnings
- Verifies all public items have docs
```

---

## Release Workflow Details

### When It Runs

Creating a git tag matching `v*` pattern:

```bash
git tag -a v0.2.0 -m "Release 0.2.0"
git push origin v0.2.0
```

Workflow automatically:

1. Builds and tests the release
2. Creates GitHub Release
3. Publishes to crates.io

### Jobs

#### 1. Create Release

```yaml
name: Create Release
- Checks out code
- Builds in release mode
- Runs full test suite
- Verifies formatting
- Runs clippy
- Extracts version from tag
- Gets changelog entry
- Creates GitHub Release
```

#### 2. Publish to crates.io

```yaml
name: Publish to crates.io
- Waits for create-release job
- Publishes with: cargo publish --token ${{ secrets.CARGO_TOKEN }}
```

---

## Local Development

### Using Docker (Recommended)

Start Postgres with schema initialization:

```bash
# Start all services
docker-compose up -d

# Wait for healthy status
docker-compose ps

# Access database
psql -h localhost -U postgres -d fraiseql_test

# View database UI
open http://localhost:8080
```

Cleanup:

```bash
docker-compose down
docker-compose down -v  # Remove volumes too
```

### Without Docker

Install Postgres 17:

```bash
# macOS
brew install postgresql@17

# Linux
sudo apt-get install postgresql-17

# Start Postgres
pg_ctl -D /usr/local/var/postgres start
```

Initialize test database:

```bash
createdb -U postgres fraiseql_test
psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql
psql -U postgres -d fraiseql_test -f tests/fixtures/seed_data.sql
```

---

## Running Tests Locally

### Unit Tests (Always runs)

```bash
cargo test --lib
```

### Integration Tests (Requires Postgres)

```bash
export POSTGRES_HOST=localhost
export POSTGRES_USER=postgres
export POSTGRES_PASSWORD=postgres
export POSTGRES_DB=fraiseql_test

cargo test --test integration -- --ignored
cargo test --test streaming_integration -- --ignored
```

### Load Tests (Requires Postgres + Schema)

```bash
# Initialize schema first
psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql
psql -U postgres -d fraiseql_test -f tests/fixtures/seed_data.sql

cargo test --test load_tests -- --ignored --nocapture
```

### All Tests

```bash
cargo test -- --ignored --nocapture
```

---

## Code Quality Checks

### Formatting

```bash
cargo fmt
cargo fmt -- --check  # Verify without changing
```

### Linting

```bash
cargo clippy
cargo clippy -- -D warnings  # Deny all warnings
```

### Security Audit

```bash
cargo audit
cargo audit --deny warnings
```

### Coverage

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html  # Generates coverage report
open tarpaulin-report.html
```

### Documentation

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo doc --open
```

---

## Making a Release

### Step 1: Prepare

Update version and changelog:

```bash
# Edit Cargo.toml
# version = "0.2.0"

# Edit CHANGELOG.md
# Add entry for new version
```

Commit:

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore: bump version to 0.2.0"
```

### Step 2: Automated Release

Use the release script:

```bash
./scripts/publish.sh 0.2.0
```

The script:

1. Validates version format
2. Checks you're on main branch
3. Pulls latest changes
4. Updates Cargo.toml
5. Builds and tests
6. Commits version bump
7. Creates git tag
8. Pushes to GitHub
9. Publishes to crates.io

### Step 3: Verify

Check that:

1. GitHub Release was created: <https://github.com/fraiseql/fraiseql-wire/releases>
2. Crates.io has new version: <https://crates.io/crates/fraiseql-wire>
3. Documentation built: <https://docs.rs/fraiseql-wire>

### Step 4: Announce

Post release notes to:

- GitHub Discussions
- Rust forums
- Social media
- Project channels

---

## GitHub Secrets Configuration

For releases to work, you need:

### CARGO_TOKEN

Required for publishing to crates.io:

1. Create API token on [crates.io](https://crates.io/me)
2. Go to GitHub repo Settings → Secrets
3. Add secret: `CARGO_TOKEN` = your token

### CODECOV_TOKEN (Optional)

For coverage reports:

1. Go to [codecov.io](https://codecov.io)
2. Enable repo
3. Copy token
4. Add secret: `CODECOV_TOKEN`

---

## Troubleshooting CI/CD

### Tests Fail in CI But Pass Locally

**Cause**: Different Postgres version or environment

**Solution**:

- Check `ci.yml` for Postgres version (postgres:15-alpine)
- Use Docker Compose to match CI environment
- Run tests with `--nocapture` to see output

### Release Workflow Fails

**Cause**: Various reasons

**Solution**:

1. Check workflow logs: GitHub repo → Actions → specific run
2. Verify `CARGO_TOKEN` is set correctly
3. Ensure `Cargo.toml` version was updated
4. Check version format matches semver

### Coverage Drops Below Target

**Cause**: New code not tested

**Solution**:

- Add tests for new code
- Run `cargo tarpaulin` locally to identify untested code
- Update coverage target if intentional

### MSRV Tests Fail

**Cause**: Using features from newer Rust version

**Solution**:

1. Identify which Rust 1.71+ feature you used
2. Replace with equivalent 1.70 compatible code
3. Re-run: `cargo +1.70 build`

---

## Best Practices

### Before Pushing

```bash
# Run all local checks
cargo fmt
cargo clippy -- -D warnings
cargo test --lib
cargo test --test integration -- --ignored
cargo audit
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

### Commit Messages

Use conventional commits:

- `feat:` New feature
- `fix:` Bug fix
- `refactor:` Code reorganization
- `test:` Tests
- `docs:` Documentation
- `chore:` Maintenance

### Pull Requests

1. Create branch from main: `git checkout -b feat/my-feature`
2. Make changes
3. Run local checks (see above)
4. Push branch
5. Create PR with description
6. Wait for CI to pass
7. Request review
8. Merge when approved

### Releases

1. Update CHANGELOG.md (before running script)
2. Run `./scripts/publish.sh 0.2.0`
3. Verify GitHub Release and crates.io
4. Add detailed notes to GitHub Release

---

## Performance

### Build Times

- Clean build: ~2-3 minutes
- Incremental build: ~30 seconds
- With caching: ~1-2 minutes

### Test Times

- Unit tests: ~30 seconds
- Integration tests: ~2 minutes
- All tests: ~3-5 minutes

### Coverage

- Tarpaulin coverage: ~2 minutes

---

## Monitoring

### GitHub Actions Status

- **Main branch**: All jobs must pass
- **PR**: All jobs must pass before merge
- **Tags**: Release job runs on push

### Codecov Coverage

- Badge shows coverage percentage
- Check coverage report: codecov.io
- Set target coverage in workflow

### crates.io

- New versions auto-appear
- Docs built automatically
- Check yanked versions if needed

---

## Related Documentation

- **TESTING_GUIDE.md**: How to run tests locally
- **TROUBLESHOOTING.md**: Common issues and fixes
- **CONTRIBUTING.md**: Contributing guidelines
- **Cargo.toml**: Package and dependency info

---

## Quick Reference

| Task | Command |
|------|---------|
| Run all checks locally | `cargo fmt && cargo clippy -- -D warnings && cargo test --lib && cargo audit` |
| Run tests with Postgres | `docker-compose up -d && cargo test -- --ignored` |
| Create a release | `./scripts/publish.sh 0.2.0` |
| Check coverage | `cargo tarpaulin --out Html` |
| View docs | `cargo doc --open` |
| Start dev environment | `docker-compose up -d` |
| Stop dev environment | `docker-compose down` |

---

**CI/CD is set up to ensure quality and reliability!** ✅
