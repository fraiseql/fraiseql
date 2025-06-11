# CQRS-Based DDL Generation: Maintaining Command/Query Separation

## Problem Statement

Automatic DDL generation shouldn't couple command-side table structures with query-side view definitions. We need approaches that respect CQRS boundaries while still providing automation benefits.

## Alternative Approaches

### 1. Separate Command and Query Models

```python
# Command side - focused on write operations
@command_model
@table("tb_users")
class UserCommand:
    """Defines storage structure for writes"""
    id: UUID = field(primary_key=True)
    email: str = field(unique=True)
    profile_data: dict = field(jsonb=True)
    version: int = field(default=1)

    class Meta:
        # Only command-side concerns
        indexes = ["email"]
        constraints = ["email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Z|a-z]{2,}$'"]

# Query side - focused on read operations
@fraise_type
@query_model
class UserQuery:
    """Defines GraphQL schema and view structure"""
    id: UUID
    email: str
    name: str = fraise_field(source="profile_data->>'name'")
    posts: List['PostQuery'] = fraise_field(
        source="v_user_posts",  # Reference to separate view
        description="User's posts"
    )
```

### 2. Event-Sourced DDL Generation

```python
# Events define the source of truth
@event
class UserCreated:
    user_id: UUID
    email: str
    profile: dict
    timestamp: datetime

@event
class UserProfileUpdated:
    user_id: UUID
    profile: dict
    timestamp: datetime

# Command side: Event store (append-only)
class EventStoreGenerator:
    def generate_ddl(self) -> str:
        return """
        CREATE TABLE tb_events (
            id BIGSERIAL PRIMARY KEY,
            aggregate_id UUID NOT NULL,
            aggregate_type TEXT NOT NULL,
            event_type TEXT NOT NULL,
            event_data JSONB NOT NULL,
            event_timestamp TIMESTAMPTZ NOT NULL,
            version INTEGER NOT NULL
        );

        CREATE INDEX idx_events_aggregate ON tb_events(aggregate_id, version);
        """

# Query side: Projections (regeneratable)
@projection(source=[UserCreated, UserProfileUpdated])
class UserProjection:
    """Materialized view built from events"""
    user_id: UUID
    email: str
    name: str
    last_updated: datetime
```

### 3. Repository-Driven DDL

```python
# Repository defines storage needs
class UserRepository:
    """Command-side repository with storage hints"""

    @storage_hint(
        table="tb_users",
        indexes=["email"],
        partitioned_by="created_at"
    )
    async def save(self, user: User) -> None:
        pass

    @storage_hint(
        requires_index="email"
    )
    async def find_by_email(self, email: str) -> Optional[User]:
        pass

# DDL generator analyzes repository methods
class RepositoryDDLGenerator:
    def analyze_repository(self, repo_class: Type) -> List[str]:
        """
        Generate DDL based on repository access patterns:
        - save() methods indicate write tables
        - find_by_X() methods indicate needed indexes
        - Partitioning based on query patterns
        """
        pass

# Query side remains independent
@fraise_type
class UserView:
    """Pure query model - no storage implications"""
    id: UUID
    email: str
    posts: List['PostView']
```

### 4. Migration-First Approach

```python
# Migrations are the source of truth
@migration(version="1.0.0")
class CreateUsersTable:
    """Explicit migration - no magic"""

    def up(self) -> str:
        return """
        CREATE TABLE tb_users (
            id UUID PRIMARY KEY,
            data JSONB NOT NULL,
            created_at TIMESTAMPTZ DEFAULT NOW()
        );
        """

    def down(self) -> str:
        return "DROP TABLE tb_users;"

# Query models reference existing structure
@fraise_type
@view_source("""
    SELECT id,
           jsonb_build_object(
               'id', id,
               'email', data->>'email',
               'name', data->>'name'
           ) as data
    FROM tb_users
""")
class User:
    """Query model with explicit view definition"""
    id: UUID
    email: str
    name: str
```

### 5. Schema Configuration Files

```yaml
# schema/command/users.yaml
command:
  users:
    table: tb_users
    columns:
      - name: id
        type: UUID
        primary_key: true
      - name: data
        type: JSONB
      - name: version
        type: INTEGER
    indexes:
      - columns: ["data->>'email'"]
        unique: true

# schema/query/users.yaml
query:
  users:
    source: tb_users
    view: v_users
    fields:
      id:
        source: id
      email:
        source: data->>'email'
      posts:
        source: |
          (SELECT jsonb_agg(...) FROM posts WHERE user_id = users.id)
```

### 6. Contract-Based Generation

```python
# Define contracts between command and query
@data_contract
class UserContract:
    """Defines the data contract between write and read models"""

    # Required fields that must exist in storage
    required_fields = ["id", "email", "created_at"]

    # Optional fields that may exist
    optional_fields = ["name", "profile", "settings"]

    # Computed fields (query-side only)
    computed_fields = {
        "post_count": "COUNT(*) FROM posts WHERE user_id = $.id",
        "last_active": "MAX(created_at) FROM activities WHERE user_id = $.id"
    }

# Command side implements storage for contract
@implements_contract(UserContract)
class UserCommandModel:
    table = "tb_users"
    storage_strategy = "jsonb"  # or "columns" for traditional

# Query side implements views for contract
@implements_contract(UserContract)
@fraise_type
class UserQueryModel:
    id: UUID
    email: str
    name: Optional[str]
    post_count: int = fraise_field(computed=True)
```

### 7. Aspect-Oriented DDL

```python
# Separate aspects of data management

@write_aspect
class UserWriteAspect:
    """Defines write characteristics"""
    table = "tb_users"
    write_patterns = ["single_insert", "bulk_update"]
    expected_volume = "1000/day"
    retention_policy = "5 years"

@read_aspect
class UserReadAspect:
    """Defines read characteristics"""
    access_patterns = ["by_id", "by_email", "list_with_pagination"]
    expected_qps = 1000
    cache_strategy = "redis_with_ttl"

@fraise_type
@aspects(write=UserWriteAspect, read=UserReadAspect)
class User:
    """Domain model with separate aspects"""
    id: UUID
    email: str
    name: str
```

### 8. Plugin-Based DDL Providers

```python
# DDL generation is pluggable

class DDLProvider(ABC):
    """Base class for DDL providers"""

    @abstractmethod
    def generate_command_ddl(self, models: List[Type]) -> List[str]:
        pass

    @abstractmethod
    def generate_query_ddl(self, models: List[Type]) -> List[str]:
        pass

class PostgreSQLProvider(DDLProvider):
    """PostgreSQL-specific DDL generation"""

    def generate_command_ddl(self, models: List[Type]) -> List[str]:
        # Generate tables optimized for writes
        return [
            "CREATE TABLE ... WITH (fillfactor=70)",  # Leave space for updates
            "CREATE INDEX ... WHERE status = 'active'"  # Partial indexes
        ]

    def generate_query_ddl(self, models: List[Type]) -> List[str]:
        # Generate views optimized for reads
        return [
            "CREATE MATERIALIZED VIEW ...",
            "CREATE INDEX ... USING gin(...)"  # Full-text search
        ]

# Configure in settings
DDL_PROVIDERS = {
    "command": "fraiseql.ddl.PostgreSQLProvider",
    "query": "fraiseql.ddl.TimescaleProvider"  # Different for time-series queries
}
```

### 9. Template-Based Generation

```python
# Use templates to maintain separation

# templates/command/table.sql.jinja2
CREATE TABLE {{ table_name }} (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    {% for field in fields %}
    {{ field.name }} {{ field.type }}{% if field.constraints %} {{ field.constraints }}{% endif %},
    {% endfor %}
    created_at TIMESTAMPTZ DEFAULT NOW()
);

# templates/query/view.sql.jinja2
CREATE VIEW {{ view_name }} AS
SELECT
    id,
    jsonb_build_object(
        '__typename', '{{ type_name }}',
        {% for field in fields %}
        '{{ field.graphql_name }}', {{ field.sql_expression }}{% if not loop.last %},{% endif %}
        {% endfor %}
    ) AS data
FROM {{ source_table }};

# Models just provide metadata
@fraise_type
@ddl_template("query/user_view.sql.jinja2")
class User:
    """Pure domain model"""
    id: UUID
    email: str
    name: str
```

### 10. Declarative Mapping Files

```python
# Separate mapping configuration

# mappings/user_mapping.py
class UserMapping:
    """Maps between command storage and query views"""

    command = {
        "table": "tb_users",
        "schema": {
            "id": "UUID PRIMARY KEY",
            "data": "JSONB NOT NULL",
            "version": "INTEGER DEFAULT 1"
        }
    }

    query = {
        "view": "v_users",
        "source": "tb_users",
        "fields": {
            "id": "id",
            "email": "data->>'email'",
            "name": "data->>'name'",
            "posts": "(SELECT jsonb_agg(...) FROM posts WHERE ...)"
        }
    }

# Models remain pure
@fraise_type
class User:
    id: UUID
    email: str
    name: str
    posts: List['Post']
```

## Recommended Approach

The best approach likely combines several strategies:

1. **Separate Models**: Distinct command and query models
2. **Repository Patterns**: Let repositories drive command-side DDL
3. **Explicit Views**: Query-side views defined separately
4. **Contract-Based**: Ensure consistency through contracts
5. **Pluggable Providers**: Different strategies for different use cases

This maintains CQRS separation while still providing automation benefits:
- Command side optimized for writes
- Query side optimized for reads
- No coupling between the two
- Clear boundaries and responsibilities

The key insight is that **DDL generation should respect architectural boundaries** rather than trying to generate everything from a single model.
