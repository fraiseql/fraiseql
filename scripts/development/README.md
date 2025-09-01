# Development Scripts

Scripts for daily development workflow support and local environment setup.

## Scripts Overview

### `typecheck.sh`
**Purpose**: Run type checking with pyright
**Usage**: `./typecheck.sh`
**Dependencies**: pyright (installed via npm or system package)

Performs static type analysis to catch type errors before runtime.

### `start-postgres-daemon.sh`
**Purpose**: Start local PostgreSQL daemon for development
**Usage**: `./start-postgres-daemon.sh`
**Dependencies**: PostgreSQL server

Sets up and starts PostgreSQL with development-friendly configuration.

### `test-db-setup.sh`
**Purpose**: Initialize test database with schema and data
**Usage**: `./test-db-setup.sh`
**Dependencies**: PostgreSQL running, database permissions

Creates and configures test database for development and testing.

### `claude_mcp_server.py`
**Purpose**: Claude MCP server for AI-assisted development
**Usage**: `python claude_mcp_server.py`
**Dependencies**: MCP libraries, Claude integration

Provides AI-assisted development capabilities through Claude integration.

## Workflow Integration

These scripts integrate into the standard development workflow:

1. **Environment Setup**: `start-postgres-daemon.sh` → `test-db-setup.sh`
2. **Code Quality**: `typecheck.sh` before commits
3. **AI Assistance**: `claude_mcp_server.py` for development support

## Troubleshooting

**PostgreSQL won't start**: Check if port 5432 is available
**Type checking fails**: Ensure pyright is installed and configured
**Database setup fails**: Verify PostgreSQL permissions and connectivity
