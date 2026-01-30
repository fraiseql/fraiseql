# 2.4: Type System

**Audience:** Schema designers, backend developers, API architects
**Prerequisite:** Topics 1.2 (Core Concepts), 2.1 (Compilation Pipeline)
**Reading Time:** 15-20 minutes

---

## Overview

FraiseQL's type system is the bridge between your database schema and your GraphQL API. Every column in your database maps to a type in your schema, which maps to a field in your GraphQL API.

**Key Insight:** Types are automatically inferred from your database, ensuring your GraphQL schema and database schema never get out of sync.

---

## Type System Architecture

```
Database Schema
(PostgreSQL, MySQL, SQLite, SQL Server)
         ↓
Column Types
(INT, VARCHAR, TIMESTAMP, BOOLEAN, etc.)
         ↓
FraiseQL Infers
(at compile time)
         ↓
GraphQL Scalar Types
(Int, String, DateTime, Boolean, etc.)
         ↓
API Contracts
(sent to client)
```

---

## Built-In Scalar Types

### The Mapping

| Database Type | FraiseQL Type | GraphQL Type | Example |
|---------------|---------------|--------------|---------|
| INT, SERIAL | int | Int | 123 |
| BIGINT, BIGSERIAL | long | Long | 9223372036854775807 |
| SMALLINT | short | Short | 32767 |
| FLOAT, REAL | float | Float | 3.14 |
| NUMERIC, DECIMAL | decimal | Decimal | 1234.56 |
| VARCHAR, CHAR, TEXT | string | String | "hello" |
| BOOLEAN, BOOL | boolean | Boolean | true |
| DATE | date | Date | "2026-01-29" |
| TIME | time | Time | "14:30:00" |
| TIMESTAMP | datetime | DateTime | "2026-01-29T14:30:00Z" |
| UUID | uuid | UUID | "550e8400-e29b-41d4-a716-446655440000" |
| JSON, JSONB | json | JSON | {"key": "value"} |
| BYTEA, BLOB | bytes | Bytes | (binary data) |

### Type Inference Examples

**PostgreSQL:**
```sql
CREATE TABLE tb_users (
  pk_user_id SERIAL PRIMARY KEY,              -- ← Int (non-nullable)
  email VARCHAR(255) NOT NULL,                 -- ← String (non-nullable)
  age SMALLINT,                                -- ← Short (nullable)
  balance NUMERIC(10, 2),                      -- ← Decimal (nullable)
  is_active BOOLEAN DEFAULT true,              -- ← Boolean (non-nullable)
  created_at TIMESTAMP DEFAULT NOW(),          -- ← DateTime (non-nullable)
  metadata JSONB,                              -- ← JSON (nullable)
  avatar_data BYTEA                            -- ← Bytes (nullable)
);
```

**Generated GraphQL Type:**
```graphql
type User {
  userId: Int!              # Non-nullable (PRIMARY KEY)
  email: String!            # Non-nullable (NOT NULL)
  age: Short                # Nullable (no constraint)
  balance: Decimal          # Nullable
  isActive: Boolean!        # Non-nullable (DEFAULT)
  createdAt: DateTime!      # Non-nullable (DEFAULT)
  metadata: JSON            # Nullable
  avatarData: Bytes         # Nullable
}
```

**Python Schema (Optional - explicit definition):**
```python
from fraiseql import schema
from datetime import datetime
from decimal import Decimal

@schema.type(table="tb_users")
class User:
    user_id: int              # Required (non-nullable)
    email: str                # Required
    age: int | None           # Optional (Python 3.10+)
    balance: Decimal | None   # Optional
    is_active: bool           # Required
    created_at: datetime      # Required
    metadata: dict | None     # Optional
    avatar_data: bytes | None # Optional
```

---

## Nullable vs Non-Nullable Types

### Database Constraints Drive Nullability

**Rule 1: NOT NULL → Non-Nullable in GraphQL**
```sql
CREATE TABLE tb_orders (
  pk_order_id INT PRIMARY KEY,           -- NOT NULL by default (primary key)
  fk_user_id INT NOT NULL,               -- NOT NULL constraint
  total NUMERIC(10, 2),                  -- NULL allowed (nullable)
  status VARCHAR(20) NOT NULL DEFAULT 'pending'
);
```

**Result in GraphQL:**
```graphql
type Order {
  orderId: Int!              # Non-nullable (PRIMARY KEY)
  userId: Int!               # Non-nullable (NOT NULL)
  total: Decimal             # Nullable (no constraint)
  status: String!            # Non-nullable (NOT NULL + DEFAULT)
}
```

**Rule 2: DEFAULT → Non-Nullable (if NO NULL allowed)**
```sql
CREATE TABLE tb_products (
  pk_product_id INT PRIMARY KEY,
  price NUMERIC(10, 2) NOT NULL DEFAULT 0.00,  -- Has default, NOT NULL
  discount NUMERIC(10, 2) DEFAULT 0.00         -- Has default, but NULL allowed
);
```

**Result in GraphQL:**
```graphql
type Product {
  productId: Int!     # Non-nullable (PRIMARY KEY)
  price: Decimal!     # Non-nullable (DEFAULT + NOT NULL)
  discount: Decimal   # Nullable (can be NULL even with default)
}
```

### When Types Are Nullable

```graphql
# Nullable types (optional fields, user can omit in GraphQL query)
type User {
  email: String!      # Must always be present in response
  phone: String       # Can be null/absent in response
  address: String     # Can be null/absent in response
}

# In a query:
query {
  user {
    email    # Always included (non-null in schema)
    phone    # Included if set, null if not
  }
}
```

---

## Composite Types: Objects and Relationships

### Object Types

An object type is a composite type with multiple fields:

```graphql
type Order {
  orderId: Int!
  userId: Int!
  total: Decimal!
  createdAt: DateTime!
  status: String!
}
```

**Database Table:**
```sql
CREATE TABLE tb_orders (
  pk_order_id INT PRIMARY KEY,
  fk_user_id INT NOT NULL,
  total NUMERIC(10, 2) NOT NULL,
  created_at TIMESTAMP NOT NULL,
  status VARCHAR(20) NOT NULL
);
```

### Relationships: One-to-Many

**Database Schema:**
```sql
CREATE TABLE tb_users (
  pk_user_id INT PRIMARY KEY,
  email VARCHAR(255) NOT NULL
);

CREATE TABLE tb_orders (
  pk_order_id INT PRIMARY KEY,
  fk_user_id INT NOT NULL REFERENCES tb_users(pk_user_id),
  total NUMERIC(10, 2) NOT NULL
);
```

**GraphQL Types with Relationship:**
```graphql
type User {
  userId: Int!
  email: String!
  orders: [Order!]!      # One-to-many: User has many Orders
}

type Order {
  orderId: Int!
  total: Decimal!
  user: User!            # Many-to-one: Order belongs to User
}
```

**Query Example:**
```graphql
query GetUserWithOrders($userId: Int!) {
  user(userId: $userId) {
    userId
    email
    orders {              # Relationship automatically resolved
      orderId
      total
    }
  }
}
```

### Relationships: Many-to-Many

**Database Schema (Junction Table):**
```sql
CREATE TABLE tb_students (
  pk_student_id INT PRIMARY KEY,
  name VARCHAR(255) NOT NULL
);

CREATE TABLE tb_courses (
  pk_course_id INT PRIMARY KEY,
  title VARCHAR(255) NOT NULL
);

CREATE TABLE tj_student_courses (
  fk_student_id INT NOT NULL REFERENCES tb_students(pk_student_id),
  fk_course_id INT NOT NULL REFERENCES tb_courses(pk_course_id),
  PRIMARY KEY (fk_student_id, fk_course_id)
);
```

**GraphQL Types with Many-to-Many:**
```graphql
type Student {
  studentId: Int!
  name: String!
  courses: [Course!]!    # Many-to-many relationship
}

type Course {
  courseId: Int!
  title: String!
  students: [Student!]!  # Many-to-many relationship
}
```

**Query Example:**
```graphql
query GetStudentCourses($studentId: Int!) {
  student(studentId: $studentId) {
    name
    courses {             # Automatically resolved through junction table
      title
    }
  }
}
```

---

## List Types

### Arrays in GraphQL

**Non-Empty List:**
```graphql
type User {
  tags: [String!]!       # List of non-null strings, list itself is non-null
}

# Valid:
{ tags: ["vip", "premium"] }

# Invalid:
{ tags: null }           # ❌ List is non-null
{ tags: ["vip", null] }  # ❌ Items must be non-null
```

**Nullable List:**
```graphql
type User {
  tags: [String!]        # List can be null, but items must be non-null
}

# Valid:
{ tags: ["vip", "premium"] }
{ tags: null }           # ✅ List can be null

# Invalid:
{ tags: ["vip", null] }  # ❌ Items can't be null
```

**List of Nullable Items:**
```graphql
type User {
  notes: [String]!       # Non-null list, but items can be null
}

# Valid:
{ notes: ["note1", "note2"] }
{ notes: ["note1", null, "note3"] }

# Invalid:
{ notes: null }          # ❌ List is non-null
```

### Database to List Mapping

**One-to-Many as List:**
```sql
CREATE TABLE tb_orders (
  pk_order_id INT PRIMARY KEY,
  fk_user_id INT NOT NULL,
  ...
);
```

**Maps to GraphQL List:**
```graphql
type User {
  orders: [Order!]!      # Automatically inferred from foreign key relationship
}
```

---

## Custom Scalar Types

While FraiseQL automatically infers types from your database, you can define custom scalars for application-specific types:

### Defining Custom Scalars

**PostgreSQL with Custom Types:**
```sql
-- Create an enum type
CREATE TYPE order_status AS ENUM ('pending', 'confirmed', 'shipped', 'delivered');

CREATE TABLE tb_orders (
  pk_order_id INT PRIMARY KEY,
  status order_status NOT NULL DEFAULT 'pending'
);
```

**FraiseQL Inference:**
```graphql
enum OrderStatus {
  PENDING
  CONFIRMED
  SHIPPED
  DELIVERED
}

type Order {
  orderId: Int!
  status: OrderStatus!   # Automatically inferred enum type
}
```

### Custom Scalar Definitions

**For complex types (in Python schema):**
```python
from fraiseql import schema
from typing import NewType
from datetime import datetime

# Define custom scalar
PhoneNumber = NewType('PhoneNumber', str)
DateRange = NewType('DateRange', dict)

@schema.type(table="tb_users")
class User:
    user_id: int
    email: str
    phone: PhoneNumber      # Custom scalar: validates phone format
    created_at: datetime
    available_dates: DateRange  # Custom scalar: complex type
```

**Validation:**
```python
@schema.scalar("PhoneNumber")
def serialize_phone(value: str) -> str:
    # Ensure phone format
    return format_phone(value)

@schema.scalar("DateRange")
def serialize_date_range(value: dict) -> dict:
    # Ensure valid date range
    return {
        "start": value["start"],
        "end": value["end"]
    }
```

---

## Type Modifiers

### Required vs Optional

**Required Field (Non-Null):**
```graphql
type User {
  email: String!    # Must always have a value
}

# Query must include this field if requested
query {
  user {
    email  # ✅ Valid
  }
}
```

**Optional Field (Nullable):**
```graphql
type User {
  phone: String     # Can be null
}

# Query can skip or get null
query {
  user {
    phone  # ✅ Valid (might return null)
  }
}
```

### List Modifiers

```graphql
[String]        # List can be null, items can be null
[String!]       # List can be null, items non-null
[String]!       # List non-null, items can be null
[String!]!      # List non-null, items non-null
```

---

## Type Safety in Action

### Compile-Time Type Validation

**Schema Definition:**
```python
@schema.type(table="tb_users")
class User:
    user_id: int
    email: str
```

**Compile-Time Checks:**
```
✅ user_id: column pk_user_id is INT → type int ✓
✅ email: column email is VARCHAR → type str ✓
✅ PRIMARY KEY constraint → non-null ✓
✅ NOT NULL constraint on email → non-null ✓
✅ All types match database schema ✓
```

### Runtime Type Validation

**Query Execution:**
```graphql
query GetUser($userId: Int!) {
  user(userId: $userId) {
    userId
    email
  }
}

Variables: { "userId": "not-a-number" }
```

**Runtime Check:**
```
❌ Variable $userId: expected Int, got String
Error: "Variable $userId of type Int! was not provided a valid Int value"
```

---

## Type Inference Examples

### Example 1: E-Commerce Product Type

**Database:**
```sql
CREATE TABLE tb_products (
  pk_product_id SERIAL PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  description TEXT,
  price NUMERIC(10, 2) NOT NULL,
  stock_quantity INT NOT NULL DEFAULT 0,
  is_featured BOOLEAN DEFAULT false,
  created_at TIMESTAMP NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);
```

**Inferred GraphQL Type:**
```graphql
type Product {
  productId: Int!              # SERIAL primary key
  name: String!                # VARCHAR NOT NULL
  description: String          # TEXT (nullable)
  price: Decimal!              # NUMERIC NOT NULL
  stockQuantity: Int!          # INT NOT NULL DEFAULT
  isFeatured: Boolean!         # BOOLEAN DEFAULT
  createdAt: DateTime!         # TIMESTAMP NOT NULL DEFAULT
  updatedAt: DateTime!         # TIMESTAMP NOT NULL DEFAULT
}
```

**Optional Python Schema (explicit):**
```python
@schema.type(table="tb_products")
class Product:
    product_id: int
    name: str
    description: str | None
    price: Decimal
    stock_quantity: int
    is_featured: bool
    created_at: datetime
    updated_at: datetime
```

### Example 2: Complex User Type with Relationships

**Database:**
```sql
CREATE TABLE tb_users (
  pk_user_id SERIAL PRIMARY KEY,
  email VARCHAR(255) NOT NULL UNIQUE,
  first_name VARCHAR(100),
  last_name VARCHAR(100),
  created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE tb_orders (
  pk_order_id SERIAL PRIMARY KEY,
  fk_user_id INT NOT NULL REFERENCES tb_users(pk_user_id),
  total NUMERIC(10, 2) NOT NULL,
  created_at TIMESTAMP NOT NULL
);
```

**Inferred GraphQL Types:**
```graphql
type User {
  userId: Int!
  email: String!
  firstName: String              # VARCHAR without NOT NULL
  lastName: String               # VARCHAR without NOT NULL
  createdAt: DateTime!
  orders: [Order!]!              # One-to-many relationship
}

type Order {
  orderId: Int!
  userId: Int!
  total: Decimal!
  createdAt: DateTime!
  user: User!                    # Many-to-one relationship
}
```

---

## Type System Benefits

### 1. Automatic Consistency
```
Database Schema (source of truth)
         ↓
Compile-time Inference
         ↓
GraphQL Schema (always in sync)
         ↓
No manual type synchronization needed
```

### 2. Safety Guarantees
```graphql
type Order {
  total: Decimal!    # Guaranteed non-null
                     # Database enforces NOT NULL
                     # GraphQL enforces non-null
                     # Application can rely on it
}
```

### 3. Self-Documenting API
```graphql
# From this type definition alone, you know:
type Order {
  orderId: Int!        # Always present, always an integer
  total: Decimal!      # Always present, always a number
  notes: String        # May be null/absent
  createdAt: DateTime! # Always present, always a timestamp
  user: User!          # Always present, always a User object
}
```

---

## Type System Best Practices

### 1. Use Explicit Nullability

```graphql
# ❌ Avoid ambiguity
type User {
  email: String       # Is this null when user has no email?
}

# ✅ Be explicit
type User {
  email: String!      # Always present
  phone: String       # Can be null (user didn't provide)
}
```

### 2. Use Relationships Over Foreign Keys

```graphql
# ❌ Expose raw foreign key
type Order {
  orderId: Int!
  userId: Int!        # User must fetch user separately
}

# ✅ Provide relationship
type Order {
  orderId: Int!
  user: User!         # Automatic relationship resolution
}
```

### 3. Name Fields Clearly

```graphql
# ❌ Unclear
type User {
  a: String!
  b: Int!
}

# ✅ Clear intent
type User {
  email: String!
  age: Int!
}
```

### 4. Use Enums for Constrained Values

```graphql
# ❌ Free-form string
type Order {
  status: String!     # Could be anything
}

# ✅ Enum for clarity
enum OrderStatus {
  PENDING
  CONFIRMED
  SHIPPED
  DELIVERED
}

type Order {
  status: OrderStatus!  # Limited to defined values
}
```

---

## Related Topics

- **Topic 1.2:** Core Concepts & Terminology (understanding types)
- **Topic 2.1:** Compilation Pipeline (how types are inferred)
- **Topic 2.2:** Query Execution Model (type validation at runtime)
- **Topic 2.5:** Error Handling & Validation (type error handling)
- **Topic 3.1:** Python Schema Authoring (defining types explicitly)

---

## Summary

FraiseQL's type system is automatically inferred from your database schema:

**Built-In Scalar Types:**
- Integers: Int, Long, Short
- Decimals: Decimal, Float
- Strings: String
- Dates/Times: Date, Time, DateTime
- Booleans: Boolean
- Special: UUID, JSON, Bytes

**Key Principles:**
1. **Database constraints drive nullability** - NOT NULL in DB = non-null in GraphQL
2. **Relationships are automatic** - Foreign keys become GraphQL relationships
3. **Lists are inferred** - One-to-many becomes [Type!]!
4. **Type safety guaranteed** - Compile-time and runtime validation
5. **Self-documenting** - Schema clearly shows what's required vs optional

**Benefits:**
- No manual type synchronization
- Database and GraphQL schema always in sync
- Type safety at compile-time and runtime
- Self-documenting API contracts
- Clear nullability semantics
