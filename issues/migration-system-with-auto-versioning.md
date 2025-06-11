# Language-Native Migration System with Automated Schema Versioning

## Summary

Implement a minimal, language-native migration system for FraiseQL that uses only Python and SQL, avoiding heavy dependencies like Alembic. The system should include automated schema versioning through git hooks and semantic version detection.

## Current State

The blog example shows a basic pattern:
- Numbered SQL files (`001_initial_schema.sql`, etc.)
- Manual execution via bash scripts
- No version tracking or rollback support
- No schema versioning strategy

## Proposed Solution

A minimal migration system that:
1. Uses numbered SQL files for migrations
2. Tracks applied migrations in PostgreSQL
3. Provides a simple Python runner (using only psycopg)
4. Automatically detects and versions schema changes
5. Generates compatibility views for breaking changes

## Core Components

### 1. Migration File Structure

```
migrations/
├── 001_initial_schema.sql
├── 002_add_users_table.sql
├── 003_create_views.sql
├── schema-version.json      # Auto-maintained version manifest
└── README.md
```

### 2. Migration Tracking Table

```sql
CREATE TABLE IF NOT EXISTS _fraiseql_migrations (
    version INTEGER PRIMARY KEY,
    filename TEXT NOT NULL,
    schema_version VARCHAR(20),
    applied_at TIMESTAMPTZ DEFAULT NOW(),
    changes JSONB  -- Breaking changes, new features, etc.
);

CREATE TABLE IF NOT EXISTS _fraiseql_schema_versions (
    version VARCHAR(20) PRIMARY KEY,
    parent_version VARCHAR(20),
    changes JSONB NOT NULL,
    compatibility_views JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### 3. Minimal Python Migration Runner

```python
# fraiseql/migrations/runner.py
import os
import re
import json
from pathlib import Path
from typing import List, Tuple, Optional
import asyncpg
from datetime import datetime

class MigrationRunner:
    def __init__(self, connection_url: str, migrations_path: str = "migrations"):
        self.connection_url = connection_url
        self.migrations_path = Path(migrations_path)

    async def ensure_migrations_table(self, conn):
        """Create migrations tracking tables if they don't exist."""
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS _fraiseql_migrations (
                version INTEGER PRIMARY KEY,
                filename TEXT NOT NULL,
                schema_version VARCHAR(20),
                applied_at TIMESTAMPTZ DEFAULT NOW(),
                changes JSONB
            );

            CREATE TABLE IF NOT EXISTS _fraiseql_schema_versions (
                version VARCHAR(20) PRIMARY KEY,
                parent_version VARCHAR(20),
                changes JSONB NOT NULL,
                compatibility_views JSONB,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
        """)

    async def get_applied_migrations(self, conn) -> set[int]:
        """Get set of already applied migration versions."""
        rows = await conn.fetch("SELECT version FROM _fraiseql_migrations")
        return {row['version'] for row in rows}

    def get_migration_files(self) -> List[Tuple[int, str]]:
        """Get sorted list of (version, filename) tuples."""
        migrations = []
        pattern = re.compile(r'^(\d{3,4})_.*\.sql$')

        for file in self.migrations_path.glob("*.sql"):
            match = pattern.match(file.name)
            if match:
                version = int(match.group(1))
                migrations.append((version, file.name))

        return sorted(migrations)

    async def get_current_schema_version(self, conn) -> Optional[str]:
        """Get the current schema version."""
        row = await conn.fetchrow("""
            SELECT version FROM _fraiseql_schema_versions
            ORDER BY created_at DESC LIMIT 1
        """)
        return row['version'] if row else None

    async def run_migrations(self, target_version: Optional[str] = None):
        """Run all pending migrations."""
        conn = await asyncpg.connect(self.connection_url)
        try:
            await self.ensure_migrations_table(conn)
            applied = await self.get_applied_migrations(conn)
            migrations = self.get_migration_files()

            # Load schema version manifest
            manifest_path = self.migrations_path / "schema-version.json"
            manifest = {}
            if manifest_path.exists():
                manifest = json.loads(manifest_path.read_text())

            for version, filename in migrations:
                if version not in applied:
                    print(f"Applying migration {filename}...")

                    # Read and execute migration
                    sql = (self.migrations_path / filename).read_text()

                    # Extract schema version from migration if present
                    schema_version = self._extract_schema_version(sql)
                    changes = self._extract_changes(sql)

                    # Execute in transaction
                    async with conn.transaction():
                        await conn.execute(sql)

                        # Record migration
                        await conn.execute("""
                            INSERT INTO _fraiseql_migrations
                            (version, filename, schema_version, changes)
                            VALUES ($1, $2, $3, $4)
                        """, version, filename, schema_version, json.dumps(changes))

                        if schema_version:
                            await self._record_schema_version(
                                conn, schema_version, changes
                            )

                    print(f"✓ Applied {filename}")

                    if target_version and schema_version == target_version:
                        print(f"Reached target version {target_version}")
                        break

        finally:
            await conn.close()

    def _extract_schema_version(self, sql: str) -> Optional[str]:
        """Extract schema version from SQL comments."""
        match = re.search(r'-- Schema Version: ([\d.]+)', sql)
        return match.group(1) if match else None

    def _extract_changes(self, sql: str) -> dict:
        """Extract change information from SQL comments."""
        changes = {'breaking': [], 'features': [], 'fixes': []}

        # Look for change markers in comments
        for line in sql.split('\n'):
            if '-- BREAKING:' in line:
                changes['breaking'].append(line.split('BREAKING:')[1].strip())
            elif '-- FEATURE:' in line:
                changes['features'].append(line.split('FEATURE:')[1].strip())
            elif '-- FIX:' in line:
                changes['fixes'].append(line.split('FIX:')[1].strip())

        return changes

    async def _record_schema_version(self, conn, version: str, changes: dict):
        """Record a new schema version."""
        compatibility_views = {}

        # Auto-detect compatibility views needed
        if changes.get('breaking'):
            # Generate compatibility view names
            major_version = version.split('.')[0]
            prev_major = str(int(major_version) - 1)

            # This would be enhanced by schema differ
            compatibility_views = {
                f"v{prev_major}": "auto-generated compatibility views"
            }

        await conn.execute("""
            INSERT INTO _fraiseql_schema_versions
            (version, parent_version, changes, compatibility_views)
            VALUES ($1, $2, $3, $4)
        """, version, None, json.dumps(changes), json.dumps(compatibility_views))
```

### 4. Automated Schema Version Detection

```python
# fraiseql/migrations/versioning.py
import ast
import difflib
from typing import Dict, List, Tuple
from pathlib import Path
from dataclasses import dataclass
from enum import Enum

class ChangeType(Enum):
    BREAKING = "breaking"  # Major version bump
    FEATURE = "feature"    # Minor version bump
    FIX = "fix"           # Patch version bump

@dataclass
class SchemaChange:
    type: ChangeType
    description: str
    entity: str
    field: Optional[str] = None
    old_value: Optional[str] = None
    new_value: Optional[str] = None

class SchemaVersionDetector:
    def __init__(self, old_path: str, new_path: str):
        self.old_types = self._extract_fraiseql_types(old_path)
        self.new_types = self._extract_fraiseql_types(new_path)

    def detect_changes(self) -> List[SchemaChange]:
        """Detect all schema changes between versions."""
        changes = []

        # Check for removed types (BREAKING)
        for type_name in self.old_types:
            if type_name not in self.new_types:
                changes.append(SchemaChange(
                    type=ChangeType.BREAKING,
                    description=f"Removed type {type_name}",
                    entity=type_name
                ))

        # Check for new types (FEATURE)
        for type_name in self.new_types:
            if type_name not in self.old_types:
                changes.append(SchemaChange(
                    type=ChangeType.FEATURE,
                    description=f"Added type {type_name}",
                    entity=type_name
                ))

        # Check for field changes in existing types
        for type_name in set(self.old_types) & set(self.new_types):
            old_fields = self.old_types[type_name]
            new_fields = self.new_types[type_name]

            # Removed fields (BREAKING)
            for field_name, field_type in old_fields.items():
                if field_name not in new_fields:
                    changes.append(SchemaChange(
                        type=ChangeType.BREAKING,
                        description=f"Removed field {type_name}.{field_name}",
                        entity=type_name,
                        field=field_name
                    ))

            # Added fields (FEATURE if optional, BREAKING if required)
            for field_name, field_type in new_fields.items():
                if field_name not in old_fields:
                    is_optional = 'Optional' in str(field_type) or '| None' in str(field_type)
                    changes.append(SchemaChange(
                        type=ChangeType.FEATURE if is_optional else ChangeType.BREAKING,
                        description=f"Added {'optional' if is_optional else 'required'} field {type_name}.{field_name}",
                        entity=type_name,
                        field=field_name
                    ))

            # Changed field types (BREAKING)
            for field_name in set(old_fields) & set(new_fields):
                if old_fields[field_name] != new_fields[field_name]:
                    changes.append(SchemaChange(
                        type=ChangeType.BREAKING,
                        description=f"Changed type of {type_name}.{field_name}",
                        entity=type_name,
                        field=field_name,
                        old_value=str(old_fields[field_name]),
                        new_value=str(new_fields[field_name])
                    ))

        return changes

    def _extract_fraiseql_types(self, path: str) -> Dict[str, Dict[str, str]]:
        """Extract all FraiseQL type definitions from Python files."""
        types = {}

        for py_file in Path(path).rglob("*.py"):
            try:
                tree = ast.parse(py_file.read_text())
                for node in ast.walk(tree):
                    if isinstance(node, ast.ClassDef):
                        # Check if it's a FraiseQL type
                        if self._is_fraiseql_type(node):
                            fields = self._extract_fields(node)
                            types[node.name] = fields
            except:
                continue

        return types

    def _is_fraiseql_type(self, node: ast.ClassDef) -> bool:
        """Check if a class is decorated with @fraiseql.type or similar."""
        for decorator in node.decorator_list:
            if isinstance(decorator, ast.Name) and decorator.id in ['type', 'fraise_type']:
                return True
            if isinstance(decorator, ast.Attribute) and decorator.attr in ['type', 'fraise_type']:
                return True
        return False

    def _extract_fields(self, node: ast.ClassDef) -> Dict[str, str]:
        """Extract field definitions from a class."""
        fields = {}

        for item in node.body:
            if isinstance(item, ast.AnnAssign) and isinstance(item.target, ast.Name):
                field_name = item.target.id
                field_type = ast.unparse(item.annotation) if hasattr(ast, 'unparse') else str(item.annotation)
                fields[field_name] = field_type

        return fields

    def calculate_version_bump(self, changes: List[SchemaChange], current_version: str) -> str:
        """Calculate new version based on semantic versioning rules."""
        major, minor, patch = map(int, current_version.split('.'))

        has_breaking = any(c.type == ChangeType.BREAKING for c in changes)
        has_features = any(c.type == ChangeType.FEATURE for c in changes)
        has_fixes = any(c.type == ChangeType.FIX for c in changes)

        if has_breaking:
            return f"{major + 1}.0.0"
        elif has_features:
            return f"{major}.{minor + 1}.0"
        elif has_fixes:
            return f"{major}.{minor}.{patch + 1}"
        else:
            return current_version
```

### 5. Git Hook for Automatic Versioning

```bash
#!/bin/bash
# .git/hooks/pre-commit

# Detect schema changes
python -m fraiseql.migrations.detect_changes

# If changes detected, generate migration
if [ $? -eq 0 ]; then
    NEW_VERSION=$(cat .fraiseql-version-bump)
    MIGRATION_NUM=$(ls migrations/*.sql | wc -l | xargs printf "%03d")
    MIGRATION_FILE="migrations/${MIGRATION_NUM}_schema_v${NEW_VERSION}.sql"

    # Generate migration with compatibility views
    python -m fraiseql.migrations.generate_migration \
        --version "$NEW_VERSION" \
        --output "$MIGRATION_FILE"

    # Add migration to git
    git add "$MIGRATION_FILE"
    git add migrations/schema-version.json

    echo "Generated migration for schema v${NEW_VERSION}"
fi
```

### 6. Auto-Generated Migration Example

```sql
-- Auto-generated migration
-- Schema Version: 2.0.0
-- Generated: 2024-01-15T10:30:00Z
-- BREAKING: Renamed User.fullName to User.name
-- BREAKING: Renamed User.emailAddress to User.email
-- FEATURE: Added User.profile field

-- Create compatibility view for v1 clients
CREATE OR REPLACE VIEW v_users_v1 AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'fullName', data->>'name',  -- Map new name to old
        'emailAddress', data->>'email'  -- Map new name to old
    ) as data
FROM users;

-- Update main view to v2 structure
CREATE OR REPLACE VIEW v_users AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', data->>'name',  -- New field name
        'email', data->>'email',  -- New field name
        'profile', data->'profile'  -- New field
    ) as data
FROM users;

-- Create view alias for explicit v2 access
CREATE OR REPLACE VIEW v_users_v2 AS SELECT * FROM v_users;

-- Record schema version change
INSERT INTO _fraiseql_schema_versions (version, parent_version, changes, compatibility_views)
VALUES (
    '2.0.0',
    '1.0.0',
    '{
        "breaking": [
            "Renamed User.fullName to User.name",
            "Renamed User.emailAddress to User.email"
        ],
        "features": ["Added User.profile field"]
    }'::jsonb,
    '{
        "v1": ["v_users_v1"],
        "v2": ["v_users", "v_users_v2"]
    }'::jsonb
);
```

### 7. CLI Integration

```bash
# Run migrations
python -m fraiseql migrate

# Run migrations to specific version
python -m fraiseql migrate --version 2.0.0

# Show migration status
python -m fraiseql migrate status

# Generate migration from schema changes
python -m fraiseql migrate generate

# Show schema version history
python -m fraiseql schema history

# Validate client compatibility
python -m fraiseql schema validate --client-version 1.0.0
```

### 8. Version Negotiation in Application

```python
# In fraiseql/fastapi/app.py
class FraiseQLApp:
    def get_view_for_type(self, type_name: str, client_version: Optional[str] = None) -> str:
        """Get appropriate view based on client version."""
        if client_version:
            # Extract major version
            major_version = client_version.split('.')[0]
            versioned_view = f"v_{type_name.lower()}_v{major_version}"

            # Check if versioned view exists
            if self.view_exists(versioned_view):
                return versioned_view

        # Default to latest
        return f"v_{type_name.lower()}"

# Client specifies version via:
# 1. Header: X-Schema-Version: 1.0.0
# 2. Query param: /graphql?version=1.0.0
```

## Benefits

1. **Zero Dependencies** - Uses only psycopg (already required)
2. **Language Native** - Pure Python + SQL
3. **Automated Versioning** - Git hooks detect schema changes
4. **Backward Compatibility** - Auto-generated compatibility views
5. **LLM-Friendly** - Simple patterns, clear conventions
6. **Production Ready** - Version negotiation, migration rollback points
7. **Type-Safe** - Changes detected from Python type definitions

## Migration Best Practices

1. **JSONB First** - Most changes don't require table alterations
2. **View-Based Evolution** - Update views instead of tables when possible
3. **Function-Based Logic** - Business logic in PostgreSQL functions
4. **Forward-Only** - No complex rollback mechanisms
5. **Automated Compatibility** - Let the system generate compatibility layers

## Implementation Phases

### Phase 1: Basic Migration Runner
- Migration tracking table
- Simple Python runner
- CLI commands

### Phase 2: Schema Version Detection
- AST-based type extraction
- Change detection algorithm
- Version calculation

### Phase 3: Automation
- Git hooks
- Migration generation
- Compatibility view generation

### Phase 4: Version Negotiation
- Client version headers
- View selection logic
- Deprecation warnings

## Example Workflow

1. Developer changes `@fraiseql.type` class
2. Git hook detects changes on commit
3. System analyzes change impact (breaking/feature/fix)
4. Version automatically bumped (2.0.0 → 2.1.0)
5. Migration generated with:
   - New view definitions
   - Compatibility views for breaking changes
   - Version metadata
6. CI runs migration in test environment
7. Production deployment applies migration
8. Old clients continue working via compatibility views

This system would make FraiseQL truly LLM-native for schema evolution - AI assistants could modify types without worrying about versioning or compatibility, as the system handles it automatically.
