# Automatic DDL Generation and Migration Management

## Overview

Implement automatic SQL DDL generation from FraiseQL type definitions, creating a complete migration system that generates tables, views, indexes, and migration scripts from Python code.

## Core Concept

Transform FraiseQL type definitions into complete database schemas:

```python
@fraise_type
class User:
    id: UUID = fraise_field(primary_key=True)
    email: str = fraise_field(unique=True, index=True)
    name: str = fraise_field(description="Display name")
    profile: UserProfile = fraise_field(description="User profile data")
    posts: List['Post'] = fraise_field(back_populates="author")
    created_at: datetime = fraise_field(default_factory=datetime.now)
    updated_at: datetime = fraise_field(auto_update=True)
```

Should automatically generate all required DDL statements.

## 1. Table Generation

### Basic Table Structure

```python
class TableGenerator:
    """Generate CREATE TABLE statements from type definitions"""

    def generate_table(self, type_def: Type) -> str:
        """
        For each @fraise_type, generate:
        - Main table with id and JSONB data column
        - System columns (created_at, updated_at, version)
        - Check constraints for required fields
        """
        return f"""
        CREATE TABLE tb_{snake_case(type_def.__name__)} (
            -- Primary key
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

            -- JSONB data storage
            data JSONB NOT NULL DEFAULT '{{}}',

            -- System columns
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            version INTEGER NOT NULL DEFAULT 1,

            -- Ensure required fields exist
            CONSTRAINT chk_required_fields CHECK (
                data ?& ARRAY{self._get_required_fields(type_def)}
            )
        );
        """
```

### Advanced Features

```python
@fraise_type
@table_options(
    partitioned_by="created_at",
    partition_interval="month",
    indexes=["email", "status", ("created_at", "DESC")],
    unlogged=False  # For testing environments
)
class Event:
    id: UUID
    type: str
    data: dict
    created_at: datetime
```

## 2. View Generation

### Query Views

```python
class ViewGenerator:
    """Generate views for GraphQL queries"""

    def generate_query_view(self, type_def: Type, include_relations: bool = True) -> str:
        """
        Generate views that:
        - Transform JSONB to GraphQL-ready format
        - Include related entities (if specified)
        - Handle computed fields
        - Support different view strategies
        """
        if include_relations:
            return self._generate_full_view(type_def)
        else:
            return self._generate_base_view(type_def)

    def _generate_full_view(self, type_def: Type) -> str:
        """Generate view with all relationships pre-loaded"""
        # Include subqueries for related entities
        # Use jsonb_agg for collections
        # Handle recursive relationships
        pass
```

### View Strategies

```python
@fraise_type
@view_strategies({
    "default": ViewStrategy.BASE,      # Minimal fields
    "full": ViewStrategy.FULL,         # All relations
    "list": ViewStrategy.LIST,         # Optimized for lists
    "detail": ViewStrategy.DETAIL,     # Optimized for single record
})
class User:
    # Different views for different use cases
    pass
```

## 3. Index Generation

### Smart Index Creation

```python
class IndexGenerator:
    """Generate indexes based on field attributes and usage patterns"""

    def generate_indexes(self, type_def: Type) -> List[str]:
        """
        Create indexes for:
        - Fields marked with index=True
        - Unique constraints
        - Foreign key relationships
        - Common query patterns (detected from GraphQL queries)
        """
        indexes = []

        for field_name, field_info in self._get_fields(type_def):
            if field_info.index:
                # JSONB expression index
                indexes.append(f"""
                CREATE INDEX idx_{table}_{field_name}
                ON tb_{table} ((data->>'{field_name}'))
                """)

            if field_info.unique:
                # Unique index on JSONB field
                indexes.append(f"""
                CREATE UNIQUE INDEX uniq_{table}_{field_name}
                ON tb_{table} ((data->>'{field_name}'))
                WHERE data->>'{field_name}' IS NOT NULL
                """)

        return indexes
```

### Composite and Partial Indexes

```python
@fraise_type
class Order:
    status: str = fraise_field(
        index=True,
        partial_index="status IN ('pending', 'processing')"
    )
    created_at: datetime = fraise_field(index=True)
    total: Decimal = fraise_field(
        index=True,
        partial_index="total > 1000"
    )

# Generates:
# CREATE INDEX idx_order_status ON tb_orders ((data->>'status'))
#   WHERE data->>'status' IN ('pending', 'processing');
# CREATE INDEX idx_order_high_value ON tb_orders ((data->>'total')::decimal)
#   WHERE (data->>'total')::decimal > 1000;
```

## 4. Constraint Generation

### Check Constraints

```python
@fraise_type
class Product:
    price: Decimal = fraise_field(
        constraints=[
            CheckConstraint("price > 0", name="positive_price"),
            CheckConstraint("price < 1000000", name="max_price")
        ]
    )
    status: str = fraise_field(
        choices=["active", "inactive", "discontinued"]
    )

# Generates:
# ALTER TABLE tb_products ADD CONSTRAINT positive_price
#   CHECK ((data->>'price')::decimal > 0);
# ALTER TABLE tb_products ADD CONSTRAINT valid_status
#   CHECK (data->>'status' IN ('active', 'inactive', 'discontinued'));
```

## 5. Foreign Key Relationships

### Reference Management

```python
class RelationshipGenerator:
    """Generate foreign key constraints and junction tables"""

    def generate_relationships(self, type_def: Type) -> List[str]:
        """
        Handle:
        - One-to-many: Store FK in child JSONB
        - Many-to-many: Create junction tables
        - Polymorphic: Use discriminator columns
        """
        sqls = []

        for field_name, field_type in self._get_relationships(type_def):
            if self._is_many_to_many(field_type):
                # Create junction table
                sqls.append(self._create_junction_table(type_def, field_type))
            # Note: One-to-many handled via JSONB foreign keys

        return sqls
```

## 6. Migration Generation

### Automatic Migration Detection

```python
class MigrationGenerator:
    """Detect schema changes and generate migrations"""

    def detect_changes(self,
        old_schema: Dict[str, Type],
        new_schema: Dict[str, Type]
    ) -> List[Migration]:
        """
        Compare schemas and detect:
        - New types (tables)
        - Removed types
        - Field additions/removals
        - Type changes
        - Constraint modifications
        """
        migrations = []

        # New tables
        for type_name in new_schema - old_schema:
            migrations.append(CreateTableMigration(new_schema[type_name]))

        # Modified tables
        for type_name in new_schema & old_schema:
            changes = self._compare_types(
                old_schema[type_name],
                new_schema[type_name]
            )
            if changes:
                migrations.append(AlterTableMigration(type_name, changes))

        return migrations
```

### Migration Script Generation

```python
class MigrationScript:
    """Generate executable migration scripts"""

    def generate(self, migrations: List[Migration]) -> str:
        """
        Create migration script with:
        - Transaction boundaries
        - Rollback statements
        - Data migration logic
        - Version tracking
        """
        return f"""
        -- Migration: {self.version}
        -- Generated: {datetime.now()}

        BEGIN;

        -- Forward migration
        {self._generate_up_statements(migrations)}

        -- Update schema version
        INSERT INTO schema_versions (version, applied_at)
        VALUES ('{self.version}', NOW());

        COMMIT;

        -- Rollback script
        -- BEGIN;
        -- {self._generate_down_statements(migrations)}
        -- DELETE FROM schema_versions WHERE version = '{self.version}';
        -- COMMIT;
        """
```

## 7. Schema Evolution Tracking

### Version Management

```python
@dataclass
class SchemaVersion:
    """Track schema versions and compatibility"""
    version: str  # Semantic version
    checksum: str  # Hash of all type definitions
    applied_at: datetime
    migration_script: str

class SchemaEvolution:
    """Manage schema evolution over time"""

    def calculate_version_bump(self, changes: List[Change]) -> str:
        """
        Determine version bump based on changes:
        - MAJOR: Breaking changes (field removal, type change)
        - MINOR: New features (field addition, new types)
        - PATCH: Bug fixes (constraint changes, index updates)
        """
        if any(c.is_breaking for c in changes):
            return self._bump_major()
        elif any(c.is_feature for c in changes):
            return self._bump_minor()
        else:
            return self._bump_patch()
```

## 8. CLI Integration

### Management Commands

```bash
# Generate initial schema
fraiseql-ddl generate --output schema.sql

# Detect changes and create migration
fraiseql-ddl makemigration --name "add_user_profile"

# Apply migrations
fraiseql-ddl migrate

# Show migration status
fraiseql-ddl status

# Generate ERD diagram
fraiseql-ddl diagram --output erd.png

# Validate schema against database
fraiseql-ddl validate
```

## 9. Development Workflow

### Automatic Migration in Development

```python
class DevelopmentMigrator:
    """Auto-migrate in development mode"""

    def auto_migrate(self, connection: Connection):
        """
        In development:
        1. Compare Python types with database schema
        2. Generate migration if differences found
        3. Apply migration automatically
        4. Warn about destructive changes
        """
        if settings.ENVIRONMENT != "development":
            raise RuntimeError("Auto-migration only in development!")

        changes = self.detect_changes()
        if changes:
            if self._has_destructive_changes(changes):
                if not self._confirm_destructive():
                    return

            migration = self.generate_migration(changes)
            self.apply_migration(migration)
```

## 10. Advanced Features

### Computed Columns

```python
@fraise_type
class Order:
    items: List[OrderItem]

    # Generate computed column
    total: Decimal = fraise_field(
        computed=True,
        expression="(SELECT SUM((item->>'price')::decimal * (item->>'quantity')::int) FROM jsonb_array_elements(data->'items') AS item)"
    )

# Generates:
# ALTER TABLE tb_orders ADD COLUMN total DECIMAL
#   GENERATED ALWAYS AS (
#     (SELECT SUM((item->>'price')::decimal * (item->>'quantity')::int)
#      FROM jsonb_array_elements(data->'items') AS item)
#   ) STORED;
```

### Triggers for Consistency

```python
@fraise_type
@triggers(
    before_insert=["validate_order_items", "calculate_totals"],
    after_update=["update_inventory", "notify_changes"]
)
class Order:
    # Auto-generate trigger functions
    pass
```

## Implementation Phases

### Phase 1: Basic DDL Generation
- Table creation from types
- Simple view generation
- Basic indexes

### Phase 2: Migration System
- Change detection
- Migration script generation
- Version tracking

### Phase 3: Advanced Features
- Complex relationships
- Computed fields
- Trigger generation

### Phase 4: Developer Experience
- CLI tools
- Auto-migration in dev
- Schema visualization

## Benefits

1. **Zero Manual SQL**: All DDL generated from Python types
2. **Type Safety**: Python types drive database schema
3. **Version Control**: Schema changes tracked in Git
4. **Migration Safety**: Automatic compatibility views
5. **LLM-Friendly**: Single source of truth for schema

This system would make FraiseQL truly "migration-free" from a developer perspective while maintaining full control over database schema evolution.
