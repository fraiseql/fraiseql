# FraiseQL

[![Quality Gate](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml/badge.svg?branch=dev)](https://github.com/fraiseql/fraiseql/actions/workflows/quality-gate.yml)
[![Documentation](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/docs.yml)
[![Release](https://img.shields.io/github/v/release/fraiseql/fraiseql)](https://github.com/fraiseql/fraiseql/releases/latest)
[![Python](https://img.shields.io/badge/Python-3.13+-blue.svg)](https://www.python.org/downloads/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Version Status](https://img.shields.io/badge/Status-Production%20Stable-green.svg)](https://github.com/fraiseql/fraiseql/blob/main/dev/audits/version-status.md)

**📍 You are here: Main FraiseQL Framework (v1.5.0) - Production Stable**

---

## **GraphQL for the LLM era. Simple. Powerful. Rust-fast.**

PostgreSQL returns JSONB. Rust transforms it. Zero Python overhead.

```python
# Complete GraphQL API in ~15 lines
from fraiseql import type, query
from fraiseql.fastapi import create_fraiseql_app

@type(sql_source="v_user", jsonb_column="data")
class User:
    id: int
    name: str
    email: str

@query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("v_user")

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User],
    queries=[users]
)
```

**Why FraiseQL?**

- ⚡ **Rust pipeline** - No Python JSON overhead, compiled performance
- 🔒 **Secure by design** - Explicit field contracts prevent data leaks
- 🤖 **AI-native** - LLMs generate correct code on first try
- 💰 **Save $5-48K/year** - Eliminate Redis, Sentry, APM tools
- 🔄 **GraphQL Cascade** - Automatic cache updates and side effect tracking
- 🔍 **Advanced filtering** - Full-text search, JSONB queries, array operations, regex
- 🧠 **Vector search** - pgvector integration for semantic search, RAG, recommendations (6 distance operators)

## 🤔 Is this for me?

**FraiseQL is for production teams** building high-performance GraphQL APIs with PostgreSQL.

### ✅ You should use FraiseQL if you:

- Build customer-facing APIs with PostgreSQL
- Need sub-millisecond query performance
- Want enterprise-grade security and monitoring
- Have 2-50 developers on your team
- Are tired of Python serialization overhead

### ❌ Consider alternatives if you:

- Need multi-database support (FraiseQL is PostgreSQL-only)
- Are building your first GraphQL API (start with simpler frameworks)
- Don't use JSONB columns in PostgreSQL

*See [detailed audience guide](dev/architecture/audiences.md) for complete user profiles.*

---

## ⚡ The Rust Advantage

**The problem with traditional GraphQL frameworks:**

```
PostgreSQL → Rows → ORM deserialize → Python objects → GraphQL serialize → JSON → Response
                    ╰────────────── Unnecessary roundtrip ──────────────╯
```

**FraiseQL's exclusive Rust pipeline:**

```
PostgreSQL → JSONB → Rust field selection → HTTP Response
             ╰──────── Zero Python overhead ────────╯
```

### Why This Matters

**No Python serialization overhead:**

```python
# Traditional framework (Strawberry + SQLAlchemy)
user = db.query(User).first()        # SQL query
user_dict = user.__dict__             # Python object → dict
json_str = json.dumps(user_dict)      # dict → JSON string (slow!)

# FraiseQL
SELECT data FROM v_user LIMIT 1       # Returns JSONB
# Rust transforms JSONB → HTTP response (7-10x faster than Python)
```

**Architectural benefits:**

- **PostgreSQL composes JSONB once** - No N+1 query problems
- **Rust selects fields** - Respects GraphQL query shape in compiled code
- **Direct HTTP response** - Zero-copy path from database to client
- **No ORM abstraction** - Database returns final data structure

**Security benefits:**

- **Explicit field exposure** - Only fields in JSONB view are accessible (no accidental leaks)
- **Clear data contracts** - JSONB structure defines exactly what's exposed
- **No ORM over-fetching** - Can't accidentally expose hidden columns
- **SQL injection protection** - PostgreSQL prepared statements + typed parameters
- **Audit trail by design** - Every mutation function can log explicitly
- **No mass assignment risks** - Input types define allowed fields precisely

**Other frameworks can't do this.** They're locked into Python-based serialization because ORM returns Python objects. ORMs can accidentally expose fields you didn't mean to serialize, or fetch entire rows when only requesting specific fields.

FraiseQL is database-first, so data is already JSON. **Rust just makes it fast and secure.**

---

## 🔒 Security by Architecture

Traditional ORM-based frameworks have inherent security risks:

### The ORM Security Problem

```python
# Traditional ORM (SQLAlchemy + Strawberry)
class User(Base):
    id = Column(Integer, primary_key=True)
    email = Column(String)
    password_hash = Column(String)  # Sensitive!
    is_admin = Column(Boolean)      # Sensitive!
    api_key = Column(String)        # Sensitive!

# Strawberry type
@strawberry.type
class UserType:
    id: int
    email: str
    # Developer forgot to exclude password_hash, is_admin, api_key!

# Risk: ORM object has ALL columns accessible
# One mistake in serialization = data leak
```

**Common ORM vulnerabilities:**

- ❌ **Accidental field exposure** - ORM loads all columns, easy to forget exclusions
- ❌ **Mass assignment attacks** - ORM objects can be updated with any field
- ❌ **Over-fetching** - Fetching entire rows increases attack surface
- ❌ **Hidden relationships** - Lazy loading can expose unintended data
- ❌ **Implicit behavior** - ORM magic makes security audits difficult

### FraiseQL's Explicit Security

```sql
-- PostgreSQL view explicitly defines what's exposed
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email
        -- password_hash, is_admin, api_key NOT included
        -- Impossible to accidentally expose them!
    ) as data
FROM tb_user;
```

```python
# Python type mirrors EXACT view structure
@type(sql_source="v_user", jsonb_column="data")
class User:
    id: int
    email: str
    # That's it. No other fields exist in this contract.
```

**FraiseQL security advantages:**

- ✅ **Explicit field whitelisting** - Only fields in JSONB view can be queried
- ✅ **Impossible to over-fetch** - View defines the complete data structure
- ✅ **Fixed recursion depth** - View defines max nesting, prevents depth attacks
- ✅ **Protected against N+1 bombs** - One query regardless of GraphQL complexity
- ✅ **Clear audit trail** - Database view + Python type = two-layer verification
- ✅ **SQL injection protection** - Prepared statements + typed parameters always
- ✅ **Mass assignment prevention** - Input types define allowed fields precisely
- ✅ **Row-level security** - PostgreSQL RLS integrates directly with views
- ✅ **Cryptographic audit logging** - Built-in SHA-256 + HMAC audit chains

### Recursion Depth Attack Protection

**Traditional GraphQL vulnerability:**

```graphql
# Malicious query - can crash traditional servers
query {
  user(id: 1) {
    posts {           # 10 posts
      author {        # → 10 queries
        posts {       # → 10 × 10 = 100 queries
          author {    # → 100 queries
            posts {   # → 1,000 queries
              # ... 10 levels = 10^10 queries = server crash
            }
          }
        }
      }
    }
  }
}
```

**Traditional framework response:**

- Each resolver level executes database queries
- N+1 problem multiplies exponentially with depth
- Requires query complexity middleware (can be bypassed)
- DataLoader reduces but doesn't eliminate the problem

**FraiseQL's built-in protection:**

```sql
-- View defines MAXIMUM recursion depth
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'posts', (
            SELECT jsonb_agg(jsonb_build_object(
                'id', p.id,
                'title', p.title
                -- NO 'author' field here!
                -- Recursion is STRUCTURALLY IMPOSSIBLE
            ))
            FROM tb_post p
            WHERE p.user_id = tb_user.id
            LIMIT 100  -- Hard limit on array size
        )
    ) as data
FROM tb_user;
```

**What happens when attacker tries deep query:**

```graphql
query {
  user {
    posts {
      author {  # ← GraphQL schema validation FAILS
        # Field 'author' doesn't exist on Post type
        # because v_post view doesn't include it
      }
    }
  }
}
```

**Protection layers:**

1. **Schema validation** - GraphQL rejects queries for non-existent fields
2. **View structure** - Database defines allowed nesting depth
3. **Hard limits** - LIMIT clauses prevent array size attacks
4. **One query** - PostgreSQL executes entire JSONB in single query

**Result:** Attackers cannot exceed the depth you define in views. No middleware needed.

### Mutation Security Example

```sql
CREATE OR REPLACE FUNCTION fn_update_user_email(
    p_user_id UUID,
    p_new_email TEXT
) RETURNS JSONB AS $$
BEGIN
    -- Explicit validation (visible in code)
    IF p_new_email !~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$' THEN
        RETURN jsonb_build_object('success', false, 'error', 'Invalid email');
    END IF;

    -- Only updates the email column (nothing else is possible)
    UPDATE tb_user
    SET email = p_new_email
    WHERE id = p_user_id;

    -- Automatic audit logging
    INSERT INTO audit_log (action, user_id, details, timestamp)
    VALUES ('email_updated', p_user_id, jsonb_build_object('new_email', p_new_email), NOW());

    RETURN jsonb_build_object('success', true);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

**No ORM magic. No hidden behavior. Everything is explicit and auditable.**

---

## 🤖 Built for AI-Assisted Development

FraiseQL is the first GraphQL framework designed for the LLM era.

### Clear Context in SQL Functions

```sql
CREATE OR REPLACE FUNCTION fn_create_user(
    p_email TEXT,
    p_name TEXT
) RETURNS JSONB AS $$
DECLARE
    v_user_id UUID;
BEGIN
    -- AI can see exactly what happens here
    -- No hidden ORM magic, no abstraction layers

    -- Validate email
    IF p_email !~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$' THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Invalid email format'
        );
    END IF;

    -- Insert user
    INSERT INTO tb_user (email, name)
    VALUES (p_email, p_name)
    RETURNING id INTO v_user_id;

    -- Log for observability
    INSERT INTO audit_log (action, details, timestamp)
    VALUES ('user_created', jsonb_build_object('user_id', v_user_id), NOW());

    -- Return clear JSONB contract
    RETURN jsonb_build_object(
        'success', true,
        'user_id', v_user_id,
        'message', 'User created successfully'
    );
END;
$$ LANGUAGE plpgsql;
```

**The entire business logic is in one place.** LLMs don't need to guess about hidden ORM behavior.

### Explicit Contracts

```python
@input
class CreateUserInput:
    email: str  # AI sees exact input structure
    name: str

@success
class UserCreated:
    user_id: str  # AI sees success response
    message: str

@failure
class ValidationError:
    error: str    # AI sees failure cases
    code: str = "VALIDATION_ERROR"

@mutation(function="fn_create_user", schema="public")
class CreateUser:
    input: CreateUserInput
    success: UserCreated
    failure: ValidationError

# That's it! FraiseQL automatically:
# 1. Calls public.fn_create_user(input) with input as dict
# 2. Parses JSONB result into UserCreated or ValidationError
```

### Why AI Loves This

- ✅ **SQL + Python** - Massively trained languages (no proprietary DSLs)
- ✅ **JSONB everywhere** - Clear data structures, obvious contracts
- ✅ **Database functions** - Complete context in one file
- ✅ **Explicit logging** - AI can trace execution without debugging
- ✅ **No abstraction layers** - What you see is what executes

**Real Impact:** Claude Code, GitHub Copilot, and ChatGPT generate correct FraiseQL code on first try.

---

## 📖 Core Concepts

**New to FraiseQL?** Understanding these core concepts will help you make the most of the framework:

**[📚 Concepts & Glossary](https://github.com/fraiseql/fraiseql/blob/main/docs/core/concepts-glossary.md)** - Essential terminology and mental models:

- **CQRS Pattern** - Separate read models (views) from write models (functions)
- **Trinity Identifiers** - Three-tier ID system (`pk_*`, `id`, `identifier`) for performance and UX
- **JSONB Views** - PostgreSQL composes data once, eliminating N+1 queries
- **Database-First Architecture** - Start with PostgreSQL, GraphQL follows
- **Explicit Sync Pattern** - Table views (`tv_*`) for complex queries

**Quick links:**

- [Understanding FraiseQL](https://github.com/fraiseql/fraiseql/blob/main/docs/guides/understanding-fraiseql.md) - 10-minute architecture overview
- [Database API](https://github.com/fraiseql/fraiseql/blob/main/docs/core/database-api.md) - Connection pooling and query execution
- [Types and Schema](https://github.com/fraiseql/fraiseql/blob/main/docs/core/types-and-schema.md) - Complete type system guide
- [Filter Operators](https://github.com/fraiseql/fraiseql/blob/main/docs/advanced/filter-operators.md) - Advanced PostgreSQL filtering (arrays, full-text search, JSONB, regex)

---

## ✨ See How Simple It Is

### Complete CRUD API in 20 Lines

```python
from uuid import UUID
from fraiseql import type, query, mutation, input, success
from fraiseql.fastapi import create_fraiseql_app

# Step 1: Map PostgreSQL view to GraphQL type
@type(sql_source="v_note", jsonb_column="data")
class Note:
    id: UUID
    title: str
    content: str | None

# Step 2: Define queries
@query
async def notes(info) -> list[Note]:
    """Get all notes."""
    db = info.context["db"]
    return await db.find("v_note")

@query
async def note(info, id: UUID) -> Note | None:
    """Get a note by ID."""
    db = info.context["db"]
    return await db.find_one("v_note", id=id)

# Step 3: Define mutations
@input
class CreateNoteInput:
    title: str
    content: str | None = None

@mutation
class CreateNote:
    input: CreateNoteInput
    success: Note

# Step 4: Create app
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[Note],
    queries=[notes, note],
    mutations=[CreateNote]
)
```

**That's it.** Your GraphQL API is ready.

### The Database-First Pattern

```sql
-- Step 1: PostgreSQL view composes data as JSONB
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'posts', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', p.id,
                    'title', p.title,
                    'content', p.content
                )
            )
            FROM tb_post p
            WHERE p.user_id = tb_user.id
        )
    ) as data
FROM tb_user;
```

```python
# Step 2: Python decorator maps it to GraphQL
@type(sql_source="v_user", jsonb_column="data")
class User:
    id: int
    name: str
    email: str
    posts: list[Post]  # Nested relations! No N+1 queries!

# Step 3: Query it
@query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("v_user")
```

**No ORM. No complex resolvers. PostgreSQL composes data, Rust transforms it.**

### Mutations with Business Logic

```sql
CREATE OR REPLACE FUNCTION fn_publish_post(p_post_id INT) RETURNS JSONB AS $$
DECLARE
    v_post RECORD;
BEGIN
    -- Get post with user info
    SELECT p.*, u.email as user_email
    INTO v_post
    FROM tb_post p
    JOIN tb_user u ON p.user_id = u.id
    WHERE p.id = p_post_id;

    -- Validate post exists
    IF NOT FOUND THEN
        RETURN jsonb_build_object('success', false, 'error', 'Post not found');
    END IF;

    -- Validate not already published
    IF v_post.published_at IS NOT NULL THEN
        RETURN jsonb_build_object('success', false, 'error', 'Post already published');
    END IF;

    -- Update post
    UPDATE tb_post
    SET published_at = NOW()
    WHERE id = p_post_id;

    -- Log event
    INSERT INTO audit_log (action, details)
    VALUES ('post_published', jsonb_build_object('post_id', p_post_id, 'user_email', v_post.user_email));

    -- Return success
    RETURN jsonb_build_object('success', true, 'post_id', p_post_id);
END;
$$ LANGUAGE plpgsql;
```

**Business logic, validation, logging - all in the database function. Crystal clear for humans and AI.**

---

## 💰 In PostgreSQL Everything

Replace 4 services with 1 database.

### Cost Savings Calculator

| Traditional Stack | FraiseQL Stack | Annual Savings |
|-------------------|----------------|----------------|
| PostgreSQL: $50/mo | PostgreSQL: $50/mo | - |
| **Redis Cloud:** $50-500/mo | ✅ **In PostgreSQL** | **$600-6,000/yr** |
| **Sentry:** $300-3,000/mo | ✅ **In PostgreSQL** | **$3,600-36,000/yr** |
| **APM Tool:** $100-500/mo | ✅ **In PostgreSQL** | **$1,200-6,000/yr** |
| **Total: $500-4,050/mo** | **Total: $50/mo** | **$5,400-48,000/yr** |

### How It Works

**Caching (Replaces Redis)**

```python
from fraiseql.caching import PostgresCache

cache = PostgresCache(db_pool)
await cache.set("user:123", user_data, ttl=3600)

# Uses PostgreSQL UNLOGGED tables
# - No WAL overhead = fast writes
# - Shared across instances
# - TTL-based expiration
# - Pattern-based deletion
```

**Error Tracking (Replaces Sentry)**

```python
from fraiseql.monitoring import init_error_tracker

tracker = init_error_tracker(db_pool, environment="production")
await tracker.capture_exception(error, context={...})

# Features:
# - Automatic error fingerprinting and grouping
# - Full stack trace capture
# - OpenTelemetry trace correlation
# - Custom notifications (Email, Slack, Webhook)
```

**Observability (Replaces APM)**

```sql
-- All traces and metrics stored in PostgreSQL
SELECT * FROM monitoring.traces
WHERE error_id = 'error-123'
  AND trace_id = 'trace-xyz';
```

**Grafana Dashboards**
Pre-built dashboards in `grafana/` query PostgreSQL directly:

- Error monitoring dashboard
- Performance metrics dashboard
- OpenTelemetry traces dashboard

### Operational Benefits

- ✅ **70% fewer services** to deploy and monitor
- ✅ **One database to backup** (not 4 separate systems)
- ✅ **No Redis connection timeouts** or cluster issues
- ✅ **No Sentry quota surprises** or rate limiting
- ✅ **ACID guarantees** for everything (no eventual consistency)
- ✅ **Self-hosted** - full control, no vendor lock-in

---

## 🏗️ Architecture Deep Dive

### Rust-First Execution

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   GraphQL       │ →  │   PostgreSQL     │ →  │   Rust          │
│   Request       │    │   JSONB Query    │    │   Transform     │
│                 │    │                  │    │   (7-10x faster)│
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                                         ↓
                                                ┌─────────────────┐
                                                │   FastAPI       │
                                                │   HTTP Response │
                                                └─────────────────┘
```

**Unified path for all queries:**

1. **GraphQL query** arrives at FastAPI
2. **Python resolver** calls PostgreSQL view/function
3. **PostgreSQL** returns pre-composed JSONB
4. **Rust pipeline** transforms JSONB based on GraphQL selection
5. **FastAPI** returns bytes directly (zero Python serialization)

### CQRS Pattern

FraiseQL implements Command Query Responsibility Segregation:

```
┌─────────────────────────────────────┐
│         GraphQL API                 │
├──────────────────┬──────────────────┤
│   QUERIES        │   MUTATIONS      │
│   (Reads)        │   (Writes)       │
├──────────────────┼──────────────────┤
│  v_* views       │  fn_* functions  │
│  tv_* tables     │  tb_* tables     │
│  JSONB ready     │  Business logic  │
└──────────────────┴──────────────────┘
```

**Queries use views:**

- `v_*` - Real-time views with JSONB computation
- `tv_*` - Denormalized tables with generated JSONB columns (for complex queries)

**Mutations use functions:**

- `fn_*` - Business logic, validation, side effects
- `tb_*` - Base tables for data storage

**[📊 Detailed Architecture Diagrams](https://github.com/fraiseql/fraiseql/blob/main/docs/guides/understanding-fraiseql.md)**

### Key Innovations

**1. Exclusive Rust Pipeline**

- PostgreSQL → Rust → HTTP (no Python JSON processing)
- 7-10x faster JSON transformation vs Python
- No GIL contention, compiled performance

**2. JSONB Views**

- Database composes data once
- Rust selects fields based on GraphQL query
- No N+1 query problems

**3. Table Views (tv_*)**

```sql
-- Denormalized JSONB table with explicit sync
CREATE TABLE tv_user (
    id INT PRIMARY KEY,
    data JSONB NOT NULL,  -- Regular column, not generated
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Sync function populates tv_* from v_* view
CREATE FUNCTION fn_sync_tv_user(p_user_id INT) RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_user (id, data)
    SELECT id, data FROM v_user WHERE id = p_user_id
    ON CONFLICT (id) DO UPDATE SET
        data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Mutations call sync explicitly
CREATE FUNCTION fn_create_user(p_name TEXT) RETURNS JSONB AS $$
DECLARE v_user_id INT;
BEGIN
    INSERT INTO tb_user (name) VALUES (p_name) RETURNING id INTO v_user_id;
    PERFORM fn_sync_tv_user(v_user_id);  -- ← Explicit sync call
    RETURN (SELECT data FROM tv_user WHERE id = v_user_id);
END;
$$ LANGUAGE plpgsql;
```

Benefits: Instant lookups, embedded relations, explicitly synchronized

**4. Zero-Copy Response**

- Direct RustResponseBytes to FastAPI
- No Python serialization overhead
- Optimal for high-throughput APIs

---

## 🎯 How FraiseQL Is Different

### Execution Path Comparison

| Framework | Data Flow | JSON Processing | Recursion Protection | Security Model |
|-----------|-----------|-----------------|----------------------|----------------|
| **FraiseQL** | PostgreSQL JSONB → Rust → HTTP | ✅ Rust (compiled) | ✅ View-enforced | ✅ Explicit contracts |
| Strawberry + SQLAlchemy | PostgreSQL → ORM → Python dict → JSON | ❌ Python (2 steps) | ⚠️ Middleware required | ❌ ORM over-fetching risk |
| Hasura | PostgreSQL → Haskell → JSON | ⚠️ Haskell | ⚠️ Middleware required | ⚠️ Complex permission system |
| PostGraphile | PostgreSQL → Node.js → JSON | ⚠️ JavaScript | ⚠️ Middleware required | ⚠️ Plugin-based |

### FraiseQL's Unique Advantages

- ✅ **Database returns final structure** (JSONB views)
- ✅ **Rust handles field selection** (compiled performance)
- ✅ **No Python in hot path** (zero serialization overhead)
- ✅ **No ORM abstraction** (SQL functions are business logic)
- ✅ **Built-in recursion protection** (view defines max depth, no middleware needed)
- ✅ **Secure by design** (explicit field contracts prevent data leaks)
- ✅ **AI-readable** (clear contracts, full context visible)
- ✅ **PostgreSQL-native** (caching, monitoring, APQ in one database)

---

## 🎯 Advanced Features

### Automatic Persisted Queries (APQ)

Enterprise-grade APQ with pluggable storage backends:

```python
from fraiseql import FraiseQLConfig

# Memory backend (zero configuration)
config = FraiseQLConfig(apq_storage_backend="memory")

# PostgreSQL backend (multi-instance coordination)
config = FraiseQLConfig(
    apq_storage_backend="postgresql",
    apq_storage_schema="apq_cache"
)
```

**How it works:**

1. Client sends query hash instead of full query
2. FraiseQL checks storage backend for cached query
3. PostgreSQL → Rust → HTTP (same fast path)
4. Bandwidth reduction with large queries

**[⚡ APQ Details](https://github.com/fraiseql/fraiseql/blob/main/docs/diagrams/apq-cache-flow.md)**

### Specialized Type System

Advanced operators for network types, hierarchical data, ranges, and nested arrays:

```graphql
query {
  servers(where: {
    ipAddress: { eq: "192.168.1.1" }          # → ::inet casting
    port: { gt: 1024 }                        # → ::integer casting
    location: { ancestor_of: "US.CA" }        # → ltree operations
    dateRange: { overlaps: "[2024-01-01,2024-12-31)" }

    # Nested array filtering with logical operators
    printServers(where: {
      AND: [
        { operatingSystem: { in: ["Linux", "Windows"] } }
        { OR: [
            { nTotalAllocations: { gte: 100 } }
            { NOT: { ipAddress: { isnull: true } } }
          ]
        }
      ]
    }) {
      hostname operatingSystem
    }
  }) {
    id name ipAddress port
  }
}
```

**50+ Specialized Scalar Types:**

**Financial & Trading:**
- CUSIP, ISIN, SEDOL, MIC, LEI - Security identifiers
- Money, Percentage, ExchangeRate - Financial values
- CurrencyCode, StockSymbol - Trading symbols

**Network & Infrastructure:**
- IPv4, IPv6, CIDR, MACAddress - Network addresses with subnet operations
- Hostname, DomainName, Port, EmailAddress - Internet identifiers
- APIKey, HashSHA256 - Security tokens

**Geospatial & Location:**
- Coordinate, Latitude, Longitude - Geographic coordinates with distance calculations
- PostalCode, Timezone - Location data

**Business & Logistics:**
- ContainerNumber, FlightNumber, TrackingNumber, VIN - Asset identifiers
- IBAN, LicensePlate - Financial & vehicle identifiers
- PhoneNumber, LocaleCode, LanguageCode - Contact & localization

**Technical & Data:**
- UUID, JSON, Date, DateTime, Time, DateRange - Standard types with validation
- LTree - Hierarchical data with ancestor/descendant queries
- SemanticVersion, Color, MIMEType, File, Image - Specialized formats
- HTML, Markdown - Rich text content

**Advanced Filtering:** Full-text search, JSONB queries, array operations, regex, vector similarity search on all types

#### Scalar Type Usage Examples

```python
from fraiseql import type
from fraiseql.types import (
    EmailAddress, PhoneNumber, Money, Percentage,
    CUSIP, ISIN, IPv4, MACAddress, LTree, DateRange
)

@type(sql_source="v_financial_data")
class FinancialRecord:
    id: int
    email: EmailAddress           # Validated email addresses
    phone: PhoneNumber           # International phone numbers
    balance: Money               # Currency amounts with precision
    margin: Percentage           # Percentages (0.00-100.00)
    security_id: CUSIP | ISIN    # Financial instrument identifiers

@type(sql_source="v_network_devices")
class NetworkDevice:
    id: int
    ip_address: IPv4             # IPv4 addresses with subnet operations
    mac_address: MACAddress      # MAC addresses with validation
    location: LTree              # Hierarchical location paths
    maintenance_window: DateRange # Date ranges with overlap queries
```

```graphql
# Advanced filtering with specialized types
query {
  financialRecords(where: {
    balance: { gte: "1000.00" }           # Money comparison
    margin: { between: ["5.0", "15.0"] }   # Percentage range
    security_id: { eq: "037833100" }       # CUSIP validation
  }) {
    id balance margin security_id
  }

  networkDevices(where: {
    ip_address: { inSubnet: "192.168.1.0/24" }  # CIDR operations
    location: { ancestor_of: "US.CA.SF" }       # LTree hierarchy
    maintenance_window: { overlaps: "[2024-01-01,2024-12-31)" }
  }) {
    id ip_address location
  }
}
```

**[📖 Nested Array Filtering Guide](https://github.com/fraiseql/fraiseql/blob/main/docs/guides/nested-array-filtering.md)**

### Enterprise Security

```python
from fraiseql import authorized

@authorized(roles=["admin", "editor"])
@mutation
class DeletePost:
    """Only admins and editors can delete posts."""
    input: DeletePostInput
    success: DeleteSuccess
    failure: PermissionDenied

# Features:
# - Field-level authorization with role inheritance
# - Row-level security via PostgreSQL RLS
# - Unified audit logging with cryptographic chain (SHA-256 + HMAC)
# - Multi-tenant isolation
# - Rate limiting and CSRF protection
```

### Trinity Identifiers

Three types of identifiers per entity for different purposes:

```python
@fraiseql.type(sql_source="posts")
class Post(TrinityMixin):
    """
    Trinity Pattern:
    - pk_post (int): Internal SERIAL key (NOT exposed, only in database)
    - id (UUID): Public API key (exposed, stable)
    - identifier (str): Human-readable slug (exposed, SEO-friendly)
    """

    # GraphQL exposed fields
    id: UUID                  # Public API (stable, secure)
    identifier: str | None    # Human-readable (SEO-friendly, slugs)
    title: str
    content: str
    # ... other fields

    # pk_post is NOT a field - accessed via TrinityMixin.get_internal_pk()
```

**Why three?**

- **pk_\*:** Fast integer joins (PostgreSQL only, never in GraphQL schema)
- **id:** Public API stability (UUID, exposed, never changes)
- **identifier:** Human-friendly URLs (exposed, SEO, readability)

---

## 🚀 Get Started in 5 Minutes

```bash
# Install
pip install fraiseql

# Create project
fraiseql init my-api
cd my-api

# Setup database
createdb my_api
psql my_api < schema.sql

# Start server
fraiseql dev
```

**Your GraphQL API is live at <http://localhost:8000/graphql>** 🎉

### Next Steps

**📚 [First Hour Guide](https://github.com/fraiseql/fraiseql/blob/main/docs/getting-started/first-hour.md)** - Build a complete blog API (60 minutes, hands-on)
**🧠 [Understanding FraiseQL](https://github.com/fraiseql/fraiseql/blob/main/docs/guides/understanding-fraiseql.md)** - Architecture deep dive (10 minute read)
**⚡ [5-Minute Quickstart](https://github.com/fraiseql/fraiseql/blob/main/docs/getting-started/quickstart.md)** - Copy, paste, run
**📖 [Full Documentation](https://github.com/fraiseql/fraiseql/tree/main/docs)** - Complete guides and references

### Prerequisites

- **Python 3.13+** (required for Rust pipeline integration and advanced type features)
- **PostgreSQL 13+**

**[📖 Detailed Installation Guide](docs/getting-started/installation.md)** - Platform-specific instructions, troubleshooting

---

## 🚦 Is FraiseQL Right for You?

### ✅ Perfect For

- **PostgreSQL-first teams** already using PostgreSQL extensively
- **Performance-critical APIs** requiring efficient data access
- **Multi-tenant SaaS** with per-tenant isolation needs
- **Cost-conscious startups** ($5-48K annual savings vs traditional stack)
- **AI-assisted development** teams using Claude/Copilot/ChatGPT
- **Operational simplicity** - one database for everything
- **Self-hosted infrastructure** - full control, no vendor lock-in

### ❌ Consider Alternatives

- **Multi-database support** - FraiseQL is PostgreSQL-specific
- **Simple CRUD APIs** - Traditional REST may be simpler
- **Non-PostgreSQL databases** - FraiseQL requires PostgreSQL
- **Microservices** - Better for monolithic or database-per-service

---

## 🛠️ CLI Commands

```bash
# Project management
fraiseql init <name>           # Create new project
fraiseql dev                   # Development server with hot reload
fraiseql check                 # Validate schema and configuration

# Code generation
fraiseql generate schema       # Export GraphQL schema
fraiseql generate types        # Generate TypeScript definitions

# Database utilities
fraiseql sql analyze <query>   # Analyze query performance
fraiseql sql explain <query>   # Show PostgreSQL execution plan

# Vector database management
fraiseql vector list           # List all tables with vector fields
fraiseql vector inspect <table>   # Inspect vector configuration
fraiseql vector validate <table> <column>  # Validate vector data
fraiseql vector create-index <table> <column>  # Generate index SQL
```

---

## 📚 Learn More

- **[Documentation](https://fraiseql.dev)** - Complete guides and API reference
- **[Examples](https://github.com/fraiseql/fraiseql/tree/main/examples)** - Real-world applications and patterns
- **[Architecture](https://github.com/fraiseql/fraiseql/tree/main/docs/architecture)** - Design decisions and trade-offs
- **[Embeddings Workflow Guide](https://github.com/fraiseql/fraiseql/blob/main/docs/guides/embeddings-workflow.md)** - Complete RAG and vector search workflow
- **[Performance Guide](https://github.com/fraiseql/fraiseql/blob/main/docs/performance/index.md)** - Optimization strategies
  - **[Benchmark Methodology](https://github.com/fraiseql/fraiseql/blob/main/docs/benchmarks/methodology.md)** - Reproducible performance benchmarks
  - **[Reproduction Guide](https://github.com/fraiseql/fraiseql/blob/main/docs/benchmarks/methodology.md#reproduction-instructions)** - Run benchmarks yourself
- **[Troubleshooting](https://github.com/fraiseql/fraiseql/blob/main/docs/guides/troubleshooting.md)** - Common issues and solutions

---

## 🤝 Contributing

We welcome contributions! See **[CONTRIBUTING.md](CONTRIBUTING.md)** for:

- Development setup and testing
- Architecture decisions and patterns
- Code style and review process

```bash
git clone https://github.com/fraiseql/fraiseql
cd fraiseql && make setup-dev
```

---

## 🙏 Acknowledgments

FraiseQL draws inspiration from:

- **[Strawberry GraphQL](https://strawberry.rocks/)** - Excellent Python GraphQL library ("Fraise" = French for strawberry)
- **Harry Percival's "Architecture Patterns with Python"** - Clean architecture and repository patterns
- **Eric Evans' "Domain-Driven Design"** - Database-centric domain modeling
- **PostgreSQL community** - For building the world's most advanced open source database

---

## 👨‍💻 About

FraiseQL is created by **Lionel Hamayon** ([@evoludigit](https://github.com/evoludigit)), a self-taught developer and founder of [Évolution digitale](https://evolution-digitale.fr).

**Started: April 2025**

### The Origin Story

I built FraiseQL out of frustration with a stupid inefficiency: **PostgreSQL returns JSON → Python deserializes to objects → GraphQL serializes back to JSON.**

Why are we doing this roundtrip?

After years moving through Django, Flask, FastAPI, and Strawberry GraphQL with SQLAlchemy, I realized the entire approach was wrong. **Just let PostgreSQL return the JSON directly. Skip the ORM. Skip the object mapping.**

But I also wanted something designed for the **LLM era**. SQL and Python are two of the most massively trained languages—LLMs understand them natively. Why not make a framework where AI can easily get context and generate correct code?

FraiseQL is the result:

- **Database-first CQRS** where PostgreSQL does what it does best
- **Rust pipeline** for compiled performance (7-10x faster than Python JSON)
- **Python stays minimal** - just decorators and type hints
- **LLM-readable by design** - clear contracts, explicit logic

Full disclosure: I built this while compulsively preparing for scale I didn't have. But that obsession led somewhere real—**zero N+1 queries, efficient architecture, and a framework that both humans and AI can understand.**

**Connect:**

- 💼 GitHub: [@evoludigit](https://github.com/evoludigit)
- 📧 <lionel.hamayon@evolution-digitale.fr>
- 🏢 [Évolution digitale](https://evolution-digitale.fr)

**Support FraiseQL:**

- ⭐ Star [fraiseql/fraiseql](https://github.com/fraiseql/fraiseql)
- 💬 Join discussions and share feedback
- 🤝 Contribute to the project

---

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

---

## 📋 Project Navigation

### Version Overview

| Version | Location | Status | Purpose | For Users? |
|---------|----------|--------|---------|------------|
| **v1.5.0** | Root level | Production Stable | Latest stable release | ✅ Recommended |
| **Rust Pipeline** | [`fraiseql_rs/`](fraiseql_rs/) | Integrated | Included in v1.0+ | ✅ Stable |
| **v1.4.1** | Superseded | Legacy | Use v1.5.0 | ⚠️ Migrate |

**New to FraiseQL?** → **[First Hour Guide](https://github.com/fraiseql/fraiseql/blob/main/docs/getting-started/first-hour.md)** • [Project Structure](https://github.com/fraiseql/fraiseql/blob/main/docs/strategic/PROJECT_STRUCTURE.md)

**Migration Guides:**

- [v1 to v2 Migration](https://github.com/fraiseql/fraiseql/blob/main/docs/migration/v1-to-v2.md) - Unified Rust-first architecture
- [Monitoring Migration](https://github.com/fraiseql/fraiseql/blob/main/docs/production/monitoring.md) - From Redis and Sentry

**[📖 Complete Version Roadmap](https://github.com/fraiseql/fraiseql/blob/main/dev/audits/version-status.md)**

---

**Ready to build the most efficient GraphQL API in Python?**

```bash
pip install fraiseql && fraiseql init my-api
```

🚀 **PostgreSQL → Rust → Production**
