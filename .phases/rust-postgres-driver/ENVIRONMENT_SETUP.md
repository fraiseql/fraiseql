# Environment Setup: Complete Installation Guide

**Document**: Step-by-step setup for all required tools
**Created**: 2025-12-18
**Duration**: 30-45 minutes total
**Platform**: macOS, Linux, Windows (WSL2)

---

## ✅ Pre-Flight Checklist

Before you start, have these ready:

- [ ] Command-line terminal access
- [ ] Admin/sudo access on your computer
- [ ] ~5 GB free disk space (Rust + PostgreSQL)
- [ ] 30 minutes of uninterrupted time

---

## Step 1: Install Rust (15 minutes)

### macOS & Linux

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Follow the prompts (usually just press Enter)
# Then load the environment:
source $HOME/.cargo/env

# Verify installation:
rustc --version
cargo --version
```

**Expected output**:

```
rustc 1.70.0 (90c541806 2023-05-31)
cargo 1.70.0 (ec8d8dbb5 2023-04-25)
```

### Windows (WSL2)

```bash
# In WSL2 terminal
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustc --version
```

### Verify Rust Works

```bash
cargo new hello_world
cd hello_world
cargo build
cargo run
```

**Expected output**:

```
Hello, world!
```

---

## Step 2: Install PostgreSQL (10 minutes)

### macOS (Homebrew)

```bash
# Install PostgreSQL
brew install postgresql

# Start PostgreSQL service
brew services start postgresql

# Verify installation:
psql --version
psql postgres -c "SELECT version();"
```

**Expected output**:

```
PostgreSQL 15.0 on x86_64-apple-darwin22.1.0, compiled by Apple clang version 14.0.0
```

### Linux (Ubuntu/Debian)

```bash
sudo apt update
sudo apt install postgresql postgresql-contrib

# Start PostgreSQL
sudo systemctl start postgresql
sudo systemctl enable postgresql

# Verify:
psql --version
sudo -u postgres psql -c "SELECT version();"
```

### Windows (WSL2)

```bash
# In WSL2
sudo apt update
sudo apt install postgresql postgresql-contrib

# Start service
sudo service postgresql start

# Verify:
psql --version
```

### Linux/WSL2: Configure PostgreSQL Access

```bash
# Switch to postgres user to access database
sudo -u postgres psql

# Inside psql:
-- Create a test user (replace 'yourname' with your actual username)
CREATE USER yourname WITH PASSWORD 'testpassword' CREATEDB;
\q

# Test access:
psql -h localhost -U yourname -d postgres
\q
```

---

## Step 3: Verify PostgreSQL Works

**Create a test database**:

```bash
# Create database
createdb test_fraiseql

# Connect to it
psql test_fraiseql

# In psql, run:
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE
);

INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com');
SELECT * FROM users;

-- Exit
\q
```

**Expected output**:

```
 id | name  |       email
----+-------+--------------------
  1 | Alice | alice@example.com
(1 row)
```

---

## Step 4: Install Docker (Optional but Recommended)

Docker is used for test containers in Phase 0.2. You can skip this if running PostgreSQL locally, but Docker is recommended for CI/CD.

### macOS

```bash
# Option 1: Homebrew
brew install docker

# Option 2: Download Docker Desktop from https://www.docker.com/products/docker-desktop

# Verify:
docker --version
docker run hello-world
```

### Linux (Ubuntu)

```bash
sudo apt install docker.io

# Start service:
sudo systemctl start docker
sudo systemctl enable docker

# Verify:
docker --version
docker run hello-world
```

### Windows (WSL2)

```bash
# In WSL2
sudo apt install docker.io

# Start service:
sudo service docker start

# Verify:
docker --version
```

---

## Step 5: Install Required Rust Tools (5 minutes)

```bash
# Install rustfmt (code formatter)
rustup component add rustfmt

# Install clippy (linter)
rustup component add clippy

# Verify:
cargo clippy --version
cargo fmt --version
```

**Expected output**:

```
clippy 0.1.70
rustfmt 1.5.3
```

---

## Step 6: Clone FraiseQL Repository

```bash
# Clone the repo
git clone https://github.com/your-repo/fraiseql.git
cd fraiseql

# Create feature branch
git checkout -b feature/rust-postgres-driver

# Verify structure:
ls -la fraiseql_rs/
```

**Expected output**:

```
Cargo.toml
Cargo.lock
src/
tests/
```

---

## Step 7: Verify All Tools Work Together

```bash
cd fraiseql_rs

# 1. Check compilation
cargo check
# Expected: ✅ Compiling fraiseql_rs

# 2. Run Clippy
cargo clippy -- -D warnings
# Expected: ✅ Finished `release` profile [optimized] target(s)

# 3. Run tests (if any exist)
cargo test
# Expected: ✅ test result: ok

# 4. Check formatting
cargo fmt -- --check
# Expected: ✅ No output (means properly formatted)
```

If all 4 pass, you're ready! 🎉

---

## Step 8: Set Up Git Pre-commit Hooks (10 minutes)

Pre-commit hooks automatically check your code before committing.

### Install prek (Rust-based pre-commit)

```bash
# macOS
brew install j178/tap/prek

# Linux (via Cargo)
cargo install prek

# Windows (via Cargo)
cargo install prek

# Verify:
prek --version
```

### Install Hooks

```bash
cd fraiseql  # Root of repo

# Install git hooks
prek install

# Verify hooks are installed:
ls -la .git/hooks/

# Expected: pre-commit hook should exist
```

### Test the Hooks

```bash
# Run all hooks on all files:
prek run --all

# Expected: All hooks pass
```

---

## Optional: IDE Setup

### VS Code (Recommended for Beginners)

1. **Install VS Code**: https://code.visualstudio.com/
2. **Install extensions**:
   - "Rust-analyzer" by The Rust Programming Language
   - "Even Better TOML" by tamasfe

3. **Open workspace**:

   ```bash
   code fraiseql/
   ```

### IntelliJ IDEA / CLion

1. **Install CLion**: https://www.jetbrains.com/clion/
2. **Plugin**: Search for "Rust" in plugins, install official Rust plugin
3. **Open project**: File → Open → fraiseql/

---

## Troubleshooting

### "Command not found: rustc"

**Problem**: Rust not in PATH

**Fix**:

```bash
# Reload shell
source $HOME/.cargo/env

# Or restart terminal
```

### "psql: command not found"

**Problem**: PostgreSQL not installed or not in PATH

**Fix**:

```bash
# Verify PostgreSQL installed
which psql

# If not found, check installation
brew list postgresql  # macOS
sudo apt list --installed | grep postgresql  # Linux

# May need to restart terminal
```

### "error: could not compile `fraiseql_rs`"

**Problem**: Missing dependencies or old Rust version

**Fix**:

```bash
# Update Rust
rustup update

# Clean and rebuild
cd fraiseql_rs
cargo clean
cargo build
```

### "Docker: Permission denied"

**Problem**: User not in docker group

**Fix** (Linux):

```bash
sudo usermod -aG docker $USER
newgrp docker

# Verify:
docker run hello-world
```

### PostgreSQL won't start on macOS

**Problem**: Permission or service issue

**Fix**:

```bash
# Check service status
brew services list

# Try restarting
brew services restart postgresql

# Or check logs
tail -50 /usr/local/var/log/postgres.log
```

---

## Verification Checklist

Run this to verify everything is installed:

```bash
#!/bin/bash
# Save as verify_setup.sh and run: bash verify_setup.sh

echo "=== Rust ==="
rustc --version
cargo --version

echo ""
echo "=== PostgreSQL ==="
psql --version

echo ""
echo "=== Rust Tools ==="
cargo clippy --version
cargo fmt --version

echo ""
echo "=== Git ==="
git --version

echo ""
echo "=== Pre-commit ==="
prek --version

echo ""
echo "=== Test Database ==="
createdb test_verify 2>/dev/null
psql test_verify -c "SELECT 'PostgreSQL is working!'" 2>/dev/null
dropdb test_verify 2>/dev/null

echo ""
echo "✅ All tools installed!"
```

---

## Expected Disk Space Usage

| Tool | Space |
|------|-------|
| Rust toolchain | ~1.5 GB |
| Cargo dependencies | ~2 GB |
| PostgreSQL | ~500 MB |
| Docker (optional) | ~1 GB |
| **Total** | **~5 GB** |

---

## Next Steps

1. ✅ Verify all tools work
2. → Read **GLOSSARY.md** (understand terminology)
3. → Read **PREREQUISITES.md** (verify your knowledge level)
4. → Start **Phase 0.1** (Clippy configuration)

---

## Getting Help

If setup fails:

1. **Google the error message** - 90% of setup issues are documented online
2. **Check official docs**:
   - Rust: https://www.rust-lang.org/tools/install
   - PostgreSQL: https://www.postgresql.org/download/
   - Docker: https://docs.docker.com/get-docker/
3. **Ask in community**:
   - Rust: https://users.rust-lang.org/
   - PostgreSQL: https://www.postgresql.org/community/

---

**Estimated Time to Get Here**: 45 minutes
**Next Document**: GLOSSARY.md
