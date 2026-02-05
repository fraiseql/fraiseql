# Recommended Rust Development Tools

This document lists the best-in-class Rust development tools for FraiseQL v2.

## Essential Tools (Required)

### 1. Rust Toolchain

```bash
# Install rustup (Rust installer)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install stable toolchain with components
rustup toolchain install stable
rustup component add rustfmt clippy rust-analyzer llvm-tools-preview
```

### 2. Core Development Tools

```bash
# cargo-watch - Auto-recompile on file changes
cargo install cargo-watch

# cargo-nextest - Faster test runner (2-3x faster)
cargo install cargo-nextest --locked

# cargo-llvm-cov - Code coverage
cargo install cargo-llvm-cov

# cargo-audit - Security vulnerability scanner
cargo install cargo-audit

# cargo-outdated - Check for outdated dependencies
cargo install cargo-outdated

# cargo-edit - Add/remove dependencies from CLI
cargo install cargo-edit
```

## Highly Recommended Tools

### 3. Performance & Analysis

```bash
# cargo-flamegraph - CPU profiling with flame graphs
cargo install flamegraph

# cargo-bloat - Find what takes space in binary
cargo install cargo-bloat

# cargo-expand - Expand macros
cargo install cargo-expand

# cargo-asm - Inspect generated assembly
cargo install cargo-asm

# cargo-benchcmp - Compare benchmark results
cargo install cargo-benchcmp

# hyperfine - Command-line benchmarking
cargo install hyperfine
```

### 4. Code Quality

```bash
# cargo-deny - Lint dependencies (licenses, advisories, bans)
cargo install cargo-deny

# cargo-geiger - Detect unsafe usage in dependency tree
cargo install cargo-geiger

# cargo-machete - Find unused dependencies
cargo install cargo-machete

# cargo-udeps - Find unused dependencies (nightly)
cargo install cargo-udeps --locked
```

### 5. Documentation

```bash
# cargo-readme - Generate README from doc comments
cargo install cargo-readme

# mdbook - Build documentation books
cargo install mdbook

# mdbook-mermaid - Mermaid diagrams in mdbook
cargo install mdbook-mermaid
```

### 6. Database Tools

```bash
# sqlx-cli - Database migrations and tooling
cargo install sqlx-cli --no-default-features --features postgres,mysql,sqlite

# diesel_cli - Alternative ORM CLI (optional)
cargo install diesel_cli --no-default-features --features postgres
```

## Optional But Useful

### 7. Build & Deployment

```bash
# cargo-zigbuild - Cross-compilation with zig
cargo install cargo-zigbuild

# cross - Easy cross-compilation
cargo install cross

# cargo-bundle - Package as .app/.deb/.rpm
cargo install cargo-bundle

# cargo-release - Automate releases
cargo install cargo-release
```

### 8. Development Utilities

```bash
# bacon - Background Rust code checker
cargo install bacon

# cargo-modules - Visualize module structure
cargo install cargo-modules

# cargo-tree - Dependency tree visualization (now built-in)
# Use: cargo tree

# cargo-cache - Manage cargo cache
cargo install cargo-cache

# taplo-cli - TOML formatter and linter
cargo install taplo-cli
```

## Fast Linkers (Highly Recommended)

Linking is often the slowest part of compilation. Use a fast linker:

### Linux: mold

```bash
# Ubuntu/Debian
sudo apt install mold

# Or build from source
git clone https://github.com/rui314/mold.git
cd mold
make -j$(nproc)
sudo make install
```

Already configured in `.cargo/config.toml`!

### macOS: zld

```bash
brew install michaeleisel/zld/zld
```

Already configured in `.cargo/config.toml`!

### Windows: lld

```bash
# Comes with LLVM, install via chocolatey
choco install llvm
```

## IDE/Editor Extensions

### VSCode (Recommended)

Already configured in `.vscode/extensions.json`:

- **rust-lang.rust-analyzer** - Core Rust support ⭐
- **tamasfe.even-better-toml** - TOML support
- **vadimcn.vscode-lldb** - Debugging
- **serayuzgur.crates** - Cargo.toml dependency management
- **usernamehw.errorlens** - Inline error display
- **eamodio.gitlens** - Git integration

Additional recommended:

```json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "tamasfe.even-better-toml",
    "vadimcn.vscode-lldb",
    "serayuzgur.crates",
    "usernamehw.errorlens",
    "eamodio.gitlens",
    "github.copilot",              // AI code completion
    "ms-vscode.test-adapter-converter",  // Test explorer
    "hbenl.vscode-test-explorer",
    "swellaby.rust-test-adapter"
  ]
}
```

### Neovim

```lua
-- Using lazy.nvim
{
  'rust-lang/rust.vim',
  'simrat39/rust-tools.nvim',
  'neovim/nvim-lspconfig',
  'nvim-lua/plenary.nvim',
  'mfussenegger/nvim-dap',
}
```

### IntelliJ IDEA / CLion

- **IntelliJ Rust** plugin
- **TOML** plugin

## Performance Monitoring

### 9. Runtime Analysis

```bash
# valgrind - Memory profiling
sudo apt install valgrind  # Linux
brew install valgrind      # macOS

# perf - Linux performance analyzer
sudo apt install linux-tools-generic

# Instruments - macOS profiling
# Comes with Xcode

# heaptrack - Heap memory profiler
sudo apt install heaptrack
```

### 10. Continuous Profiling

```bash
# samply - Firefox Profiler integration
cargo install samply

# pprof - Google pprof format
cargo install pprof
```

## CI/CD Tools

Already configured in `.github/workflows/`:

- **GitHub Actions** - CI/CD platform
- **Codecov** - Coverage reporting
- **Dependabot** - Dependency updates
- **cargo-audit** - Security scanning

## Quick Setup Script

Save this as `tools/install_tools.sh`:

```bash
#!/bin/bash
set -e

echo "Installing essential Rust development tools..."

# Core tools
cargo install cargo-watch
cargo install cargo-nextest --locked
cargo install cargo-llvm-cov
cargo install cargo-audit
cargo install cargo-outdated
cargo install cargo-edit

# Performance tools
cargo install flamegraph
cargo install cargo-bloat
cargo install cargo-expand

# Code quality
cargo install cargo-deny
cargo install cargo-machete

# Database
cargo install sqlx-cli --no-default-features --features postgres,mysql,sqlite

# Utilities
cargo install taplo-cli
cargo install bacon

echo "✅ All tools installed!"
echo ""
echo "Optional: Install fast linker"
echo "  Linux:  sudo apt install mold"
echo "  macOS:  brew install michaeleisel/zld/zld"
```

Make it executable:

```bash
chmod +x tools/install_tools.sh
./tools/install_tools.sh
```

## Tool Usage Examples

### cargo-nextest (Faster Testing)

```bash
# Run all tests with nextest (2-3x faster)
cargo nextest run

# Run specific test
cargo nextest run test_schema

# Show test output
cargo nextest run --no-capture

# Parallel test execution
cargo nextest run --test-threads 8
```

### cargo-watch (Auto-rebuild)

```bash
# Watch and run tests
cargo watch -x test

# Watch and run checks
cargo watch -x check

# Watch with custom command
cargo watch -x 'clippy --all-targets'

# Clear screen between runs
cargo watch -c -x test
```

### cargo-llvm-cov (Coverage)

```bash
# Generate coverage report
cargo llvm-cov --all-features --workspace

# HTML report
cargo llvm-cov --all-features --workspace --html
open target/llvm-cov/html/index.html

# LCOV format for CI
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

### flamegraph (Profiling)

```bash
# Profile a binary
cargo flamegraph --bin fraiseql-cli

# Profile tests
cargo flamegraph --test test_performance

# Opens flamegraph.svg
```

### cargo-bloat (Binary Size)

```bash
# Show what takes space in binary
cargo bloat --release

# Show only crates
cargo bloat --release --crates

# Specific binary
cargo bloat --release --bin fraiseql-cli
```

### cargo-deny (Dependency Linting)

```bash
# Check licenses
cargo deny check licenses

# Check security advisories
cargo deny check advisories

# Check bans
cargo deny check bans

# Check all
cargo deny check
```

### cargo-expand (Macro Expansion)

```bash
# Expand all macros
cargo expand

# Expand specific module
cargo expand schema::compiled

# Expand and show types
cargo expand --ugly
```

### sqlx-cli (Database)

```bash
# Create migration
sqlx migrate add create_users_table

# Run migrations
sqlx migrate run

# Revert migration
sqlx migrate revert

# Check queries at compile time
cargo sqlx prepare
```

### bacon (Background Checker)

```bash
# Start bacon (auto-check on save)
bacon

# Run tests
bacon test

# Run clippy
bacon clippy
```

## Performance Optimization Tips

### 1. Faster Compilation

```toml
# In .cargo/config.toml (already configured)
[build]
jobs = -1  # Use all CPU cores

[profile.dev]
opt-level = 0  # Fast compilation
debug = true

[profile.dev.package."*"]
opt-level = 2  # Optimize dependencies
```

### 2. Faster Linking

Use mold (Linux) or zld (macOS) - already configured!

### 3. Faster Testing

```bash
# Use nextest
cargo nextest run

# Parallel execution
cargo nextest run --test-threads 8

# Skip slow tests in dev
cargo nextest run --exclude-from-test long_running
```

### 4. Incremental Compilation

```bash
# Enable (default in dev)
export CARGO_INCREMENTAL=1

# Use sccache for caching across projects
cargo install sccache
export RUSTC_WRAPPER=sccache
```

## Debugging Tools

### rust-gdb / rust-lldb

```bash
# Debug with gdb
rust-gdb target/debug/fraiseql-cli

# Debug with lldb (macOS)
rust-lldb target/debug/fraiseql-cli
```

### VSCode Debugging

Already configured in `.vscode/launch.json`!

Just press F5 to debug.

## Benchmarking Tools

### Criterion (Already Configured)

```bash
# Run benchmarks
cargo bench

# Compare with baseline
cargo bench --bench schema_benchmark -- --save-baseline before
# Make changes...
cargo bench --bench schema_benchmark -- --baseline before
```

### Hyperfine (CLI Benchmarking)

```bash
# Benchmark CLI command
hyperfine 'cargo run --release -- compile schema.json'

# Compare multiple commands
hyperfine 'cargo run --release' 'cargo run'

# Warmup runs
hyperfine --warmup 3 'cargo bench'
```

## Documentation Tools

### cargo doc

```bash
# Build and open docs
cargo doc --all-features --no-deps --open

# With private items
cargo doc --all-features --document-private-items --open
```

### mdbook

```bash
# Create documentation book
mdbook init docs-book
cd docs-book
mdbook serve --open
```

## Summary of Best Tools

| Category | Tool | Purpose |
|----------|------|---------|
| **Testing** | cargo-nextest | 2-3x faster tests |
| **Coverage** | cargo-llvm-cov | Code coverage |
| **Watching** | cargo-watch / bacon | Auto-rebuild |
| **Profiling** | flamegraph | CPU profiling |
| **Size Analysis** | cargo-bloat | Binary size |
| **Security** | cargo-audit | Vulnerability scan |
| **Dependencies** | cargo-deny | Dependency linting |
| **Linking** | mold / zld | Fast linking |
| **Database** | sqlx-cli | Migrations |
| **Formatting** | rustfmt | Code formatting |
| **Linting** | clippy | Code linting |
| **IDE** | rust-analyzer | Language server |

## Current Setup Status

✅ **Already Configured:**

- rust-analyzer (VSCode)
- rustfmt
- clippy (pedantic + deny)
- cargo-llvm-cov (in CI)
- cargo-audit (in CI)
- Fast linker config (mold/zld)
- Optimized dev builds
- Benchmark infrastructure

📦 **Recommended to Install:**

```bash
# Essential (do this now)
cargo install cargo-watch cargo-nextest cargo-edit

# Performance (very useful)
cargo install flamegraph cargo-bloat cargo-expand

# Quality (optional but good)
cargo install cargo-deny cargo-machete
```

## Resources

- **Rust Performance Book**: <https://nnethercote.github.io/perf-book/>
- **Rust API Guidelines**: <https://rust-lang.github.io/api-guidelines/>
- **Clippy Lints**: <https://rust-lang.github.io/rust-clippy/>
- **Cargo Book**: <https://doc.rust-lang.org/cargo/>
- **This Week in Rust**: <https://this-week-in-rust.org/>

---

**You now have access to the best Rust tooling available!** 🦀
