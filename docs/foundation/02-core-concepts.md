# 1.2: Core Concepts & Terminology

**Audience:** All users (developers, architects, operations)
**Prerequisite:** Topic 1.1 (What is FraiseQL?)
**Reading Time:** 15-20 minutes

---

## Overview

Before diving into FraiseQL's architecture and capabilities, you need to understand the vocabulary and mental models that underpin the system. This topic defines core concepts that appear throughout FraiseQL documentation and helps you develop the right mental model for how FraiseQL works.

**Key insight:** FraiseQL uses database-native vocabulary, not application code vocabulary. This is intentional and reflects its philosophy: databases are the source of truth, not an afterthought.

---

## Part 1: Core Terminology

### Schema

**Definition:** A complete specification of your API's structure, including all types, fields, relationships, and validation rules.

In FraiseQL, a schema is authored once (in Python or TypeScript) and defines:

- What types exist (User, Order, Product, etc.)
- What fields each type has (name, email, created_at, etc.)
- What relationships exist (User has many Orders)
- How queries and mutations work (what data can be read/written)
- Authorization rules (who can access what)

```python
# Schema definition (Python)
@FraiseQL.type
class User:
    """User in the system"""
    user_id: int              # Field: unique identifier
    username: str             # Field: text name
    email: str                # Field: email address
    orders: List[Order]       # Relationship: one user has many orders
    is_active: bool           # Field: boolean flag
    created_at: datetime      # Field: timestamp
```text

**Mental model:** Schema is a *contract* between your client and server. It says "these are the exact types, fields, and relationships available to query."

---

### Type

**Definition:** A definition of a data object with named fields of specific data types.

FraiseQL has several type categories:

**1. Object Types** - Represent entities in your domain

```python
@FraiseQL.type
class User:
    user_id: int
    username: str
    email: str

@FraiseQL.type
class Order:
    order_id: int
    total: Decimal
    created_at: datetime
```text

**2. Scalar Types** - Basic values (strings, numbers, dates, etc.)

```text
String    → text (username, email, description)
Int       → whole numbers (user_id, quantity)
Float     → decimal numbers (price, rating)
Boolean   → true/false (is_active, has_shipped)
DateTime  → timestamps (created_at, updated_at)
Date      → just dates (birthday, due_date)
UUID      → unique identifiers (tracking IDs)
Decimal   → precise decimals (prices, amounts)
JSON      → arbitrary data (metadata, config)
```text

**3. Enum Types** - Limited set of named values

```python
@FraiseQL.enum
class OrderStatus:
    PENDING = "pending"
    PROCESSING = "processing"
    SHIPPED = "shipped"
    DELIVERED = "delivered"
    CANCELLED = "cancelled"
```text

**4. Interface Types** - Shared fields across multiple types (advanced)

**5. Union Types** - "One of these types" (advanced)

**Mental model:** Types are *blueprints*. Just like a database table defines the columns and their types, a GraphQL type defines the fields and their types.

---

### Field

**Definition:** A named value within a type, with a specific data type and optional validation rules.

```python
@FraiseQL.type
class Product:
    product_id: int          # Field name: product_id, type: Int
    name: str                # Field name: name, type: String
    price: Decimal           # Field name: price, type: Decimal
    in_stock: bool           # Field name: in_stock, type: Boolean
    created_at: datetime     # Field name: created_at, type: DateTime
```text

**Field modifiers:**

```python
# Required (must always have a value)
name: str                    # Required - cannot be null

# Optional (can be null/absent)
middle_name: str | None      # Optional - can be null
nickname: Optional[str]      # Alternative Python syntax
```text

**Mental model:** Fields are *columns in a database table*. Each field has a name, type, and nullability.

---

### Query

**Definition:** A read operation that retrieves data from the system without modifying it.

```graphql
# A query is a request for data
query GetUser {
  user(id: 1) {
    user_id
    username
    email
  }
}
```text

**How it works in FraiseQL:**

1. Query is received by server
2. Server looks up pre-compiled SQL template for this query shape
3. Server binds variables (id = 1) to parameters
4. Server executes SQL on database
5. Server formats results and returns

**Mental model:** A query is a *SELECT statement*. It specifies what data you want and returns results without modifying the database.

---

### Mutation

**Definition:** A write operation that modifies data (creates, updates, or deletes).

```graphql
# A mutation modifies data
mutation CreateOrder {
  createOrder(input: {
    user_id: 1
    total: 99.99
  }) {
    order_id
    status
    created_at
  }
}
```text

**How it works in FraiseQL:**

1. Mutation is received by server
2. Authorization rules checked (can user modify this data?)
3. Validation rules checked (are all required fields present?)
4. Server looks up pre-compiled SQL template for this mutation
5. Server executes SQL on database (INSERT, UPDATE, or DELETE)
6. Server formats result and returns

**Mental model:** A mutation is an *INSERT, UPDATE, or DELETE statement*. It modifies the database and returns the modified data.

---

### Resolver

**Definition:** Logic that determines what data to return for a field or relationship.

In traditional GraphQL servers, resolvers are *custom code* you write:

```javascript
// Apollo Server - Traditional resolver (you write this)
const userResolver = async (parent, args, context) => {
  return db.query("SELECT * FROM tb_users WHERE pk_user = ?", [args.id]);
};
```text

In FraiseQL, resolvers are *automatically generated* at compile time:

```python
# FraiseQL - Resolver compiled, not written
@FraiseQL.type
class User:
    user_id: int
    username: str
    # Resolver for user_id field automatically generated
    # Resolver maps to: SELECT pk_user FROM tb_users WHERE ...
```text

**Mental model:** A resolver is the *glue between GraphQL and database*. In FraiseQL, this glue is generated and optimized at compile time, not written by hand.

---

### Relationship

**Definition:** A connection between two types, representing how data relates.

**One-to-Many** (User has many Orders):

```python
@FraiseQL.type
class User:
    user_id: int
    username: str
    orders: List[Order]  # One user → many orders

@FraiseQL.type
class Order:
    order_id: int
    total: Decimal
    fk_user: int         # Foreign key back to user
```text

**Many-to-One** (Order belongs to User):

```python
@FraiseQL.type
class Order:
    order_id: int
    total: Decimal
    user: User           # Many orders → one user
```text

**Many-to-Many** (Students enroll in Courses):

```python
@FraiseQL.type
class Student:
    student_id: int
    name: str
    courses: List[Course]  # Many students → many courses

@FraiseQL.type
class Course:
    course_id: int
    name: str
    students: List[Student]  # Many courses → many students
```text

**Self-Relationships** (Employee has manager):

```python
@FraiseQL.type
class Employee:
    employee_id: int
    name: str
    manager: Employee | None  # Self-relationship
    reports: List[Employee]   # Reverse relationship
```text

**Mental model:** Relationships are *foreign keys in databases*. They connect tables and define how data relates.

---

## Part 2: Mental Models

### Mental Model 1: "Schemas Describe Your API Contract"

A schema is a contract between client and server:

**The contract says:**

- These types exist (User, Order, Product)
- These fields are available (name, email, created_at)
- These relationships exist (User has Orders)
- These queries are available (getUser, searchProducts)
- These mutations are available (createOrder, updateUser)
- These authorization rules apply (only admins can delete users)

**The client can trust:**

- Fields won't disappear (backward compatibility)
- Fields won't change type (type safety)
- Authorization will be enforced (security)

**The server guarantees:**

- Query results match the schema (type safety)
- No N+1 queries (performance)
- Consistent performance (deterministic)

**Mental model:** Think of schema as *REST API documentation on steroids*. It's not just documentation; it's enforced by the system.

---

### Mental Model 2: "Types Map to Database Tables"

In FraiseQL, types directly correspond to database tables:

```python
# GraphQL Type
@FraiseQL.type
class User:
    user_id: int
    username: str
    email: str

# Maps directly to database table
# CREATE TABLE tb_users (
#     pk_user BIGINT PRIMARY KEY,
#     username VARCHAR(255),
#     email VARCHAR(255)
# );
```text

**Why this matters:**

| Aspect | Implication |
|--------|-------------|
| **Table names** | Prefix with `tb_` (write tables) |
| **Column names** | Use snake_case (SQL convention) |
| **Primary keys** | Named `pk_{table_singular}` |
| **Foreign keys** | Named `fk_{table_singular}` |
| **Type safety** | Python type = database column type |
| **Relationships** | Foreign keys become GraphQL relationships |

**Mental model:** Your Python/TypeScript schema is *metadata about your database*. The database is the source of truth; schema describes it.

---

### Mental Model 3: "Queries Map to SELECT Statements"

Every GraphQL query compiles to a SQL SELECT statement:

```graphql
# GraphQL Query
query GetUser {
  user(id: 1) {
    user_id
    username
    orders {
      order_id
      total
    }
  }
}
```text

Compiles to approximately:

```sql
-- Compiled SQL (simplified)
SELECT
    u.pk_user,
    u.username,
    o.pk_order,
    o.total
FROM tb_users u
LEFT JOIN tb_orders o ON u.pk_user = o.fk_user
WHERE u.pk_user = 1;
```text

**Why this matters:**

- You can predict query performance (look at the SQL)
- Complex queries use database optimization (JOINs, indexes)
- No application-level N+1 queries (database handles it)
- You understand the data flow (no magic resolvers)

**Mental model:** Think of GraphQL queries as *SQL SELECT statements written in GraphQL syntax*. The database executes them, not your application.

---

### Mental Model 4: "Mutations Map to DML Statements"

GraphQL mutations compile to SQL INSERT, UPDATE, or DELETE statements:

```graphql
# GraphQL Mutation
mutation CreateOrder {
  createOrder(input: {
    user_id: 1
    total: 99.99
  }) {
    order_id
    status
    created_at
  }
}
```text

Compiles to:

```sql
-- Compiled SQL
INSERT INTO tb_orders (fk_user, total, created_at)
VALUES (1, 99.99, CURRENT_TIMESTAMP)
RETURNING pk_order, status, created_at;
```text

**Why this matters:**

- Mutations are database transactions (ACID guarantees)
- Validation happens before SQL (prevent bad data)
- Authorization checked before mutation (security)
- Results are consistent (database returns actual values)

**Mental model:** Mutations are *transactional database operations*, not application logic.

---

### Mental Model 5: "Compilation Happens Once, Execution Happens Many Times"

FraiseQL separates **build time** from **runtime**:

**Build Time (Compilation):**

```text
Python/TypeScript Schema → Compiler → Optimized SQL Templates
                        ↓
                   schema.compiled.json
```text

At build time:

- ✅ Types validated against database
- ✅ Relationships verified
- ✅ SQL generated and optimized
- ✅ Authorization rules compiled
- ✅ All errors caught

**Runtime (Execution):**

```text
GraphQL Query → Pre-compiled SQL Template → Database → Results
             ↓
        Microseconds (no interpretation)
```text

At runtime:

- ✅ Query validated (type check)
- ✅ Authorization verified
- ✅ Parameters bound to SQL
- ✅ SQL executed on database
- ✅ Results formatted

**Why this matters:**

- Errors caught at compile time, not runtime
- No schema validation overhead at query time
- Predictable performance (no interpretation)
- Deployment is deterministic (schema → binary)

**Mental model:** *Compilation separates concerns*. Build time is for safety and optimization; runtime is for pure execution.

---

## Part 3: Database-Centric Design

### Core Principle: The Database is the Source of Truth

Traditional application architecture:

```text
Client → Application Code → ORM → Database
                    ↑
         (custom resolvers, business logic, caching)
```text

FraiseQL architecture:

```text
Client → Compiled SQL Templates → Database
         (no application code)
         (deterministic)
```text

**Why this matters:**

| Aspect | Traditional | FraiseQL |
|--------|-------------|----------|
| **Where logic lives** | Application code | Database schema |
| **Consistency** | Depends on code quality | Database enforces rules |
| **Debugging** | "Why is resolver slow?" | Look at SQL query plan |
| **Performance** | Application bottleneck | Database determines speed |
| **Data integrity** | Application validation | Database constraints |

---

### View vs Table vs Relationship

FraiseQL uses database **views** extensively:

**Write Tables** (`tb_*` prefix):

```sql
CREATE TABLE tb_users (
    pk_user BIGINT PRIMARY KEY,
    username VARCHAR(255),
    email VARCHAR(255),
    created_at TIMESTAMP
);
```text

→ Normalized, DBA-owned, source of truth

**Read Views** (`v_*` prefix):

```sql
CREATE VIEW v_user AS
SELECT
    pk_user AS user_id,
    username,
    email,
    created_at
FROM tb_users
WHERE deleted_at IS NULL;  -- Soft deletes
```text

→ Curated for GraphQL, handles soft deletes, derived fields

**Analytics Views** (`va_*` prefix):

```sql
CREATE VIEW va_user AS
SELECT
    pk_user,
    username,
    COUNT(*) OVER (PARTITION BY EXTRACT(YEAR FROM created_at)) AS users_per_year,
    created_at
FROM tb_users;
```text

→ Optimized for columnar queries (Arrow plane)

**Transaction Views** (`tv_*` prefix):

```sql
CREATE VIEW tv_user AS
SELECT * FROM tb_users;
-- Used for mutations (INSERT, UPDATE, DELETE)
```text

**Mental model:** Views are *application-facing interfaces* to database tables. Tables are DBA-owned and normalized; views are curated for different access patterns.

---

### Multi-Database Philosophy

FraiseQL supports multiple databases:

```python
# PostgreSQL (primary, most features)
@FraiseQL.database("postgresql")
class User:
    user_id: int

# MySQL (secondary)
@FraiseQL.database("mysql")
class User:
    user_id: int

# SQLite (local dev)
@FraiseQL.database("sqlite")
class User:
    user_id: int

# SQL Server (enterprise)
@FraiseQL.database("sqlserver")
class User:
    user_id: int
```text

**Why this matters:**

- Use best database for the job
- Avoid vendor lock-in
- Same schema definition works everywhere
- Database-specific optimizations transparent

**Mental model:** Database is *pluggable*. Schema describes intent; database handles implementation.

---

## Part 4: Compilation vs Runtime

### Compilation (Build Time)

**What happens:**

```text
Schema (Python/TypeScript)
    ↓
Parser (validates syntax)
    ↓
Type Resolver (maps types to database)
    ↓
SQL Generator (creates templates)
    ↓
Optimizer (improves performance)
    ↓
Validator (finds errors)
    ↓
schema.compiled.json (output artifact)
```text

**What is caught at compile time:**

- ❌ Type mismatches (User.username should be VARCHAR, not INT)
- ❌ Missing relationships (Reference to non-existent table)
- ❌ Invalid queries (Field doesn't exist on type)
- ❌ Authorization rule errors
- ❌ Constraint violations

**Example - Caught at Compile Time:**

```python
# Error: Column type mismatch
@FraiseQL.type
class User:
    user_id: str  # ❌ ERROR: database has INT, schema has str

# Compilation fails with clear error message
# "Type mismatch: User.user_id is String, but pk_user in tb_users is BIGINT"
```text

---

### Runtime (Query Execution)

**What happens:**

```text
GraphQL Query
    ↓
Parser (validate syntax - compiled already)
    ↓
Authorizer (check permissions)
    ↓
Parameter Binder (bind variables to SQL)
    ↓
SQL Executor (run on database)
    ↓
Formatter (shape results to schema)
    ↓
Response (send to client)
```text

**What is checked at runtime:**

- ✅ Authorization (does user have permission?)
- ✅ Parameter validation (is ID a valid number?)
- ✅ Constraint checks (unique violation, foreign key, etc.)
- ✅ Business logic (application-defined rules)

**Example - Checked at Runtime:**

```graphql
# Runtime check: Does user have permission?
query GetUser {
  user(id: 123) {  # Authorization: Can I see user 123?
    username
  }
}

# Error (if unauthorized): "Not authorized to view user 123"
```text

---

### Comparison: Compile vs Runtime

| Check | When | What | Who Decides |
|-------|------|------|-------------|
| **Type check** | Compile | Does field exist? | Schema |
| **Type match** | Compile | Is type correct? | Schema |
| **Relationship** | Compile | Does FK exist? | Database |
| **Authorization** | Runtime | Can user access? | Application |
| **Validation** | Runtime | Is value valid? | Application rules |
| **Constraint** | Runtime | Does database allow? | Database |

**Mental model:** *Compile time catches structural errors; runtime handles business logic.*

---

## Summary: The FraiseQL Mental Model

```text
┌─────────────────────────────────────────────────────┐
│ Your Business Domain                                │
│ (E-commerce, SaaS, Data Platform, etc.)             │
└────────────────┬────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────┐
│ Database Schema (Source of Truth)                   │
│ - tb_* tables (normalized, write)                   │
│ - v_* views (curated, read)                         │
│ - fn_* functions (business logic)                   │
└────────────────┬────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────┐
│ FraiseQL Schema (Python/TypeScript)                 │
│ @FraiseQL.type                                      │
│ - Types mirror database tables                      │
│ - Fields map to columns                             │
│ - Relationships map to foreign keys                 │
└────────────────┬────────────────────────────────────┘
                 │
         (COMPILATION HAPPENS HERE)
                 │
┌────────────────▼────────────────────────────────────┐
│ Compiled Schema (schema.compiled.json)              │
│ - Validated types                                   │
│ - Optimized SQL templates                          │
│ - Authorization rules                              │
│ - Ready for runtime                                │
└────────────────┬────────────────────────────────────┘
                 │
         (RUNTIME EXECUTES HERE)
                 │
┌────────────────▼────────────────────────────────────┐
│ GraphQL Server (Execution)                          │
│ - Validates queries                                 │
│ - Checks authorization                             │
│ - Executes SQL                                      │
│ - Returns results                                   │
└────────────────┬────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────┐
│ Client Application                                  │
│ - Uses GraphQL queries                             │
│ - Receives typed results                           │
│ - Type safe (guaranteed by schema)                 │
└─────────────────────────────────────────────────────┘
```text

---

## Key Concepts Map

**Terminology:**

- **Schema** = Full specification of your API
- **Type** = Data object definition (maps to table)
- **Field** = Named value in a type (maps to column)
- **Query** = Read operation (SELECT statement)
- **Mutation** = Write operation (INSERT/UPDATE/DELETE)
- **Resolver** = Logic connecting GraphQL to database (auto-generated)
- **Relationship** = Connection between types (foreign key)

**Mental Models:**

- Schemas are *API contracts*
- Types map to *database tables*
- Queries map to *SELECT statements*
- Mutations map to *DML statements*
- Compilation separates *safety from execution*

**Database Concepts:**

- Tables (`tb_*`) = write tables
- Views (`v_*`) = read views
- Database is *source of truth*
- Multi-database support is *pluggable*

---

## Next Steps

Now that you understand the terminology and mental models:

1. **Learn the architecture** → Topic 2.1 (Compilation Pipeline)
   - How Python schemas become compiled SQL

2. **Start authoring schemas** → Topic 3.1 (Python Schema Authoring)
   - Write your first FraiseQL schema

3. **Understand design** → Topic 1.3 (Database-Centric Architecture)
   - Why FraiseQL is built this way

---

## Related Topics

- **Topic 1.1:** What is FraiseQL? — High-level positioning
- **Topic 1.3:** Database-Centric Architecture — Why databases are central
- **Topic 2.1:** Compilation Pipeline — How compilation works
- **Topic 3.1:** Python Schema Authoring — Start writing schemas

---

## Quick Reference: FraiseQL Vocabulary

| Term | Means | Example |
|------|-------|---------|
| **Schema** | Full API specification | `@FraiseQL.type class User:` |
| **Type** | Data object definition | `class User:` |
| **Field** | Value in a type | `username: str` |
| **Query** | Read operation | `query GetUser { ... }` |
| **Mutation** | Write operation | `mutation CreateOrder { ... }` |
| **Resolver** | GraphQL ↔ DB logic | Auto-generated at compile time |
| **Relationship** | Connection between types | `orders: List[Order]` |
| **Table** | Database write table | `tb_users` |
| **View** | Database read view | `v_user` |

---

**Key Takeaway:** FraiseQL uses *database-native terminology* because databases are the source of truth. Understand the database concepts, and FraiseQL becomes intuitive.
