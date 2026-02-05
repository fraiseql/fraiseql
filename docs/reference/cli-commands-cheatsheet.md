# CLI Commands Cheat Sheet

**Status:** ✅ Production Ready
**Audience:** Developers, DevOps
**Reading Time:** 5-10 minutes
**Last Updated:** 2026-02-05

Quick reference for all `fraiseql` CLI commands and options.

## Basic Commands

### compile

Compile schema to optimized execution plan.

```bash
# Compile with defaults
fraiseql compile

# Compile specific schema file
fraiseql compile --schema ./schema.json

# Compile to specific output file
fraiseql compile --output ./dist/schema.compiled.json

# Compile with verbose output
fraiseql compile --verbose

# Compile with specific configuration
fraiseql compile --config ./fraiseql.toml
```

**Flags:**
- `--schema` - Schema file path (default: `schema.json`)
- `--output` - Output path (default: `schema.compiled.json`)
- `--config` - Configuration file path (default: `fraiseql.toml`)
- `--verbose` - Show detailed compilation output
- `--check` - Only check, don't write output
- `--target` - Target database (postgres, mysql, sqlite, sqlserver)

---

### run

Start FraiseQL server.

```bash
# Start with defaults
fraiseql run

# Start on custom port
fraiseql run --port 9000

# Start with specific schema
fraiseql run --schema ./schema.compiled.json

# Start with environment file
fraiseql run --env .env.production

# Start with federation gateway
fraiseql run --federation

# Start with federation and specific port
fraiseql run --federation --port 4000
```

**Flags:**
- `--port` - HTTP port (default: 8000)
- `--schema` - Compiled schema file (default: `schema.compiled.json`)
- `--database` - Database URL override (overrides env var)
- `--env` - Environment file (.env)
- `--federation` - Enable federation mode
- `--arrow-flight` - Enable Arrow Flight server (port 50051)
- `--debug` - Enable debug logging

---

### validate

Validate schema without compiling.

```bash
# Validate schema
fraiseql validate

# Validate specific schema
fraiseql validate --schema ./schema.json

# Validate and show issues
fraiseql validate --verbose
```

**Flags:**
- `--schema` - Schema file path
- `--verbose` - Show all validation details
- `--strict` - Fail on warnings (not just errors)

---

### introspect

Introspect database and generate schema.

```bash
# Introspect database
fraiseql introspect

# Introspect and save to file
fraiseql introspect --output ./introspected_schema.json

# Introspect with verbose output
fraiseql introspect --verbose
```

**Flags:**
- `--database` - Database URL (required)
- `--output` - Output schema file (default: stdout)
- `--verbose` - Show detailed output

---

## Database Operations

### migrate

Run database migrations.

```bash
# Run all pending migrations
fraiseql migrate

# Run migrations for specific target
fraiseql migrate --target postgres

# Show pending migrations (dry-run)
fraiseql migrate --dry-run

# Migrate to specific version
fraiseql migrate --to 20240105_v2_0_1

# Rollback last migration
fraiseql migrate --rollback
```

**Flags:**
- `--target` - Database target (postgres, mysql, sqlite, sqlserver)
- `--dry-run` - Show what would run, don't execute
- `--to` - Migrate to specific version
- `--rollback` - Undo last migration
- `--force` - Force migration (dangerous!)

---

### create-migration

Create new migration file.

```bash
# Create migration
fraiseql create-migration --name add_users_table

# Create with specific type
fraiseql create-migration --name add_column --type alter
```

**Flags:**
- `--name` - Migration name (required)
- `--type` - Migration type (create, alter, drop)

---

## Federation Commands

### federation-compose

Compose subgraph schemas into federated schema.

```bash
# Compose from config
fraiseql federation-compose

# Compose with specific gateways
fraiseql federation-compose --gateway apollo --output ./composed.graphql

# Validate composition
fraiseql federation-compose --validate
```

**Flags:**
- `--config` - Federation config file
- `--gateway` - Gateway type (apollo, federation-core)
- `--output` - Output composed schema
- `--validate` - Only validate, don't generate

---

### subgraph-publish

Publish subgraph to registry.

```bash
# Publish subgraph
fraiseql subgraph-publish

# Publish with authentication
fraiseql subgraph-publish --token ABC123

# Publish to specific registry
fraiseql subgraph-publish --registry apollo --token ABC123
```

**Flags:**
- `--token` - Authentication token
- `--registry` - Registry URL or alias
- `--subgraph` - Subgraph name (default: from config)
- `--schema` - Schema file to publish

---

## Development Commands

### dev

Start development server with hot reload.

```bash
# Start dev server
fraiseql dev

# Dev server on custom port
fraiseql dev --port 3000

# Dev server with watch
fraiseql dev --watch ./src
```

**Flags:**
- `--port` - Dev port (default: 8000)
- `--watch` - Directories to watch
- `--no-reload` - Disable hot reload

---

### test

Run tests.

```bash
# Run all tests
fraiseql test

# Run specific test file
fraiseql test --file ./tests/queries.test.ts

# Run tests matching pattern
fraiseql test --match "*user*"

# Run with coverage
fraiseql test --coverage

# Run with verbose output
fraiseql test --verbose
```

**Flags:**
- `--file` - Specific test file
- `--match` - Pattern to match test names
- `--coverage` - Generate coverage report
- `--watch` - Watch mode
- `--verbose` - Detailed output

---

### bench

Run benchmarks.

```bash
# Run benchmarks
fraiseql bench

# Run specific benchmark
fraiseql bench --name query_performance

# Save baseline
fraiseql bench --save-baseline main

# Compare to baseline
fraiseql bench --baseline main
```

**Flags:**
- `--name` - Benchmark name to run
- `--save-baseline` - Save results as baseline
- `--baseline` - Compare to baseline
- `--iterations` - Number of iterations (default: 100)

---

### lint

Check code quality.

```bash
# Lint schema
fraiseql lint

# Lint with fix
fraiseql lint --fix

# Lint specific rules
fraiseql lint --only performance,security
```

**Flags:**
- `--fix` - Auto-fix issues
- `--only` - Lint specific rules
- `--config` - Lint configuration file

---

## Project Commands

### init

Initialize new FraiseQL project.

```bash
# Initialize in current directory
fraiseql init

# Initialize with template
fraiseql init --template starter

# Initialize with specific language
fraiseql init --language python
```

**Flags:**
- `--template` - Project template (starter, enterprise, etc.)
- `--language` - Primary language (python, typescript, go, java, etc.)
- `--name` - Project name

---

### generate

Generate code from schema.

```bash
# Generate code
fraiseql generate

# Generate for specific language
fraiseql generate --language python

# Generate to specific output
fraiseql generate --output ./generated
```

**Flags:**
- `--language` - Target language (python, typescript, go, java, rust, etc.)
- `--output` - Output directory
- `--client` - Generate client code (default: true)
- `--server` - Generate server types
- `--schema` - Schema file to use

---

### format

Format schema file.

```bash
# Format schema
fraiseql format

# Format in-place
fraiseql format --write

# Check format without modifying
fraiseql format --check
```

**Flags:**
- `--write` - Write formatted output to file
- `--check` - Check if formatting needed (exit code 1 if needed)

---

## Configuration Commands

### config

Manage configuration.

```bash
# Show current config
fraiseql config show

# Set value
fraiseql config set database.host localhost

# Get specific value
fraiseql config get database.host

# Reset to defaults
fraiseql config reset
```

**Flags:**
- `--file` - Config file path (default: fraiseql.toml)

---

### env

Manage environment variables.

```bash
# Show environment variables
fraiseql env

# Set variable
fraiseql env set DATABASE_URL "postgresql://..."

# Get variable
fraiseql env get DATABASE_URL

# Load from file
fraiseql env load .env.production
```

---

## Deployment Commands

### deploy

Deploy to production.

```bash
# Deploy with defaults
fraiseql deploy

# Deploy to specific environment
fraiseql deploy --environment production

# Deploy specific service
fraiseql deploy --service users-service

# Dry-run deployment
fraiseql deploy --dry-run
```

**Flags:**
- `--environment` - Target environment (dev, staging, production)
- `--service` - Specific service to deploy
- `--dry-run` - Show what would happen
- `--rollback-on-failure` - Rollback if deployment fails

---

### health

Check system health.

```bash
# Check health
fraiseql health

# Check specific component
fraiseql health --service database

# Show verbose status
fraiseql health --verbose
```

**Flags:**
- `--service` - Check specific service
- `--verbose` - Detailed status

---

## Troubleshooting Commands

### diagnose

Diagnose issues.

```bash
# Run diagnostics
fraiseql diagnose

# Save diagnostics report
fraiseql diagnose --output diagnostics.json

# Diagnose specific component
fraiseql diagnose --component database
```

**Flags:**
- `--output` - Save to file
- `--component` - Specific component to diagnose

---

### logs

View logs.

```bash
# Show recent logs
fraiseql logs

# Follow logs (tail)
fraiseql logs --follow

# Filter by level
fraiseql logs --level error

# Show last N lines
fraiseql logs --tail 100
```

**Flags:**
- `--follow` - Follow log output (tail -f)
- `--level` - Log level (error, warn, info, debug)
- `--tail` - Number of lines to show
- `--since` - Show logs since time

---

## Utility Commands

### version

Show version information.

```bash
# Show version
fraiseql version

# Show full version info
fraiseql version --verbose
```

---

### help

Show help information.

```bash
# General help
fraiseql help

# Help for specific command
fraiseql help compile

# List all commands
fraiseql commands
```

---

## Quick Reference Table

| Command | Purpose | Example |
|---------|---------|---------|
| `compile` | Build optimized schema | `fraiseql compile` |
| `run` | Start server | `fraiseql run --port 8000` |
| `validate` | Check schema | `fraiseql validate` |
| `introspect` | Generate schema from DB | `fraiseql introspect` |
| `migrate` | Run migrations | `fraiseql migrate` |
| `federation-compose` | Compose subgraphs | `fraiseql federation-compose` |
| `dev` | Development server | `fraiseql dev` |
| `test` | Run tests | `fraiseql test --coverage` |
| `bench` | Run benchmarks | `fraiseql bench` |
| `lint` | Check code quality | `fraiseql lint --fix` |
| `generate` | Generate code | `fraiseql generate --language python` |
| `deploy` | Deploy to prod | `fraiseql deploy --environment production` |

---

## See Also

- **[Scalar Types Cheatsheet](./scalar-types-cheatsheet.md)** - Type reference
- **[WHERE Operators Cheatsheet](./where-operators-cheatsheet.md)** - Filtering syntax
- **[Configuration Parameters Cheatsheet](./configuration-parameters-cheatsheet.md)** - TOML settings
