# CLI Commands Cheat Sheet

**Status:** ✅ Production Ready
**Audience:** Developers, DevOps
**Reading Time:** 5-10 minutes
**Last Updated:** 2026-02-05

Quick reference for all `FraiseQL` CLI commands and options.

## Basic Commands

### compile

Compile schema to optimized execution plan.

```bash
# Compile with defaults
FraiseQL compile

# Compile specific schema file
FraiseQL compile --schema ./schema.json

# Compile to specific output file
FraiseQL compile --output ./dist/schema.compiled.json

# Compile with verbose output
FraiseQL compile --verbose

# Compile with specific configuration
FraiseQL compile --config ./FraiseQL.toml
```

**Flags:**

- `--schema` - Schema file path (default: `schema.json`)
- `--output` - Output path (default: `schema.compiled.json`)
- `--config` - Configuration file path (default: `FraiseQL.toml`)
- `--verbose` - Show detailed compilation output
- `--check` - Only check, don't write output
- `--target` - Target database (postgres, mysql, sqlite, sqlserver)

---

### run

Start FraiseQL server.

```bash
# Start with defaults
FraiseQL run

# Start on custom port
FraiseQL run --port 9000

# Start with specific schema
FraiseQL run --schema ./schema.compiled.json

# Start with environment file
FraiseQL run --env .env.production

# Start with federation gateway
FraiseQL run --federation

# Start with federation and specific port
FraiseQL run --federation --port 4000
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
FraiseQL validate

# Validate specific schema
FraiseQL validate --schema ./schema.json

# Validate and show issues
FraiseQL validate --verbose
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
FraiseQL introspect

# Introspect and save to file
FraiseQL introspect --output ./introspected_schema.json

# Introspect with verbose output
FraiseQL introspect --verbose
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
FraiseQL migrate

# Run migrations for specific target
FraiseQL migrate --target postgres

# Show pending migrations (dry-run)
FraiseQL migrate --dry-run

# Migrate to specific version
FraiseQL migrate --to 20240105_v2_0_1

# Rollback last migration
FraiseQL migrate --rollback
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
FraiseQL create-migration --name add_users_table

# Create with specific type
FraiseQL create-migration --name add_column --type alter
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
FraiseQL federation-compose

# Compose with specific gateways
FraiseQL federation-compose --gateway apollo --output ./composed.graphql

# Validate composition
FraiseQL federation-compose --validate
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
FraiseQL subgraph-publish

# Publish with authentication
FraiseQL subgraph-publish --token ABC123

# Publish to specific registry
FraiseQL subgraph-publish --registry apollo --token ABC123
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
FraiseQL dev

# Dev server on custom port
FraiseQL dev --port 3000

# Dev server with watch
FraiseQL dev --watch ./src
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
FraiseQL test

# Run specific test file
FraiseQL test --file ./tests/queries.test.ts

# Run tests matching pattern
FraiseQL test --match "*user*"

# Run with coverage
FraiseQL test --coverage

# Run with verbose output
FraiseQL test --verbose
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
FraiseQL bench

# Run specific benchmark
FraiseQL bench --name query_performance

# Save baseline
FraiseQL bench --save-baseline main

# Compare to baseline
FraiseQL bench --baseline main
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
FraiseQL lint

# Lint with fix
FraiseQL lint --fix

# Lint specific rules
FraiseQL lint --only performance,security
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
FraiseQL init

# Initialize with template
FraiseQL init --template starter

# Initialize with specific language
FraiseQL init --language python
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
FraiseQL generate

# Generate for specific language
FraiseQL generate --language python

# Generate to specific output
FraiseQL generate --output ./generated
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
FraiseQL format

# Format in-place
FraiseQL format --write

# Check format without modifying
FraiseQL format --check
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
FraiseQL config show

# Set value
FraiseQL config set database.host localhost

# Get specific value
FraiseQL config get database.host

# Reset to defaults
FraiseQL config reset
```

**Flags:**

- `--file` - Config file path (default: FraiseQL.toml)

---

### env

Manage environment variables.

```bash
# Show environment variables
FraiseQL env

# Set variable
FraiseQL env set DATABASE_URL "postgresql://..."

# Get variable
FraiseQL env get DATABASE_URL

# Load from file
FraiseQL env load .env.production
```

---

## Deployment Commands

### deploy

Deploy to production.

```bash
# Deploy with defaults
FraiseQL deploy

# Deploy to specific environment
FraiseQL deploy --environment production

# Deploy specific service
FraiseQL deploy --service users-service

# Dry-run deployment
FraiseQL deploy --dry-run
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
FraiseQL health

# Check specific component
FraiseQL health --service database

# Show verbose status
FraiseQL health --verbose
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
FraiseQL diagnose

# Save diagnostics report
FraiseQL diagnose --output diagnostics.json

# Diagnose specific component
FraiseQL diagnose --component database
```

**Flags:**

- `--output` - Save to file
- `--component` - Specific component to diagnose

---

### logs

View logs.

```bash
# Show recent logs
FraiseQL logs

# Follow logs (tail)
FraiseQL logs --follow

# Filter by level
FraiseQL logs --level error

# Show last N lines
FraiseQL logs --tail 100
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
FraiseQL version

# Show full version info
FraiseQL version --verbose
```

---

### help

Show help information.

```bash
# General help
FraiseQL help

# Help for specific command
FraiseQL help compile

# List all commands
FraiseQL commands
```

---

## Quick Reference Table

| Command | Purpose | Example |
|---------|---------|---------|
| `compile` | Build optimized schema | `FraiseQL compile` |
| `run` | Start server | `FraiseQL run --port 8000` |
| `validate` | Check schema | `FraiseQL validate` |
| `introspect` | Generate schema from DB | `FraiseQL introspect` |
| `migrate` | Run migrations | `FraiseQL migrate` |
| `federation-compose` | Compose subgraphs | `FraiseQL federation-compose` |
| `dev` | Development server | `FraiseQL dev` |
| `test` | Run tests | `FraiseQL test --coverage` |
| `bench` | Run benchmarks | `FraiseQL bench` |
| `lint` | Check code quality | `FraiseQL lint --fix` |
| `generate` | Generate code | `FraiseQL generate --language python` |
| `deploy` | Deploy to prod | `FraiseQL deploy --environment production` |

---

## See Also

- **[Scalar Types Cheatsheet](./scalar-types-cheatsheet.md)** - Type reference
- **[WHERE Operators Cheatsheet](./where-operators-cheatsheet.md)** - Filtering syntax
- **[Configuration Parameters Cheatsheet](../reference/cli-commands-cheatsheet.md)** - TOML settings
