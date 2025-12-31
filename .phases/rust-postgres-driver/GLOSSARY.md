# Technical Glossary: Terms & Concepts

**Document**: Quick reference for technical terminology used throughout the plan
**Created**: 2025-12-18
**Use this when**: You encounter unfamiliar terms while reading phases

---

## A

### Async/Await

**What**: Rust syntax for writing asynchronous code that looks synchronous.

**Why it matters**: Allows multiple operations to run concurrently without blocking.

**Example**:
```rust
// async fn creates a Future
async fn fetch_user(id: i32) -> User {
    database.query(id).await  // .await waits for the query
}

// Call it:
let user = fetch_user(1).await;  // await waits for result
```

**Key point**: `.await` suspends the function until the operation completes, allowing other tasks to run.

---

### Arc

**What**: "Atomic Reference Counted" - allows multiple parts of your code to own the same value.

**Why it matters**: Connection pools need to be shared across multiple requests without copying.

**Example**:
```rust
use std::sync::Arc;

let pool = Arc::new(create_pool());
// Now pool can be cloned and shared across threads
let pool_copy = Arc::clone(&pool);  // Doesn't copy data, just reference
```

**Key point**: When you drop the last Arc reference, the data is deleted automatically.

---

### Async/Sync Boundaries

**What**: Points where async code must interface with synchronous code (or vice versa).

**Why it matters**: PyO3 is synchronous, but we need async Rust code. The boundary is tricky.

**Where you'll see it**: Phase 1 (connection pool) and Phase 4 (mutations).

---

## D

### Deadpool-postgres

**What**: A connection pool library for PostgreSQL that works with async Rust.

**Why it matters**: Reuses database connections instead of creating new ones (connections are expensive).

**Alternative**: Would have to write our own pool (much harder).

**Reference**: https://docs.rs/deadpool-postgres/

---

### FFI (Foreign Function Interface)

**What**: Mechanism for calling code written in one language from another language.

**In this project**: Rust code being called from Python (via PyO3).

**Why it matters**: Requires careful type conversion and error handling at boundaries.

**Example**:
```rust
#[pyfunction]  // Makes this callable from Python
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

---

## G

### GIL (Global Interpreter Lock)

**What**: Python's lock that prevents multiple threads from executing Python code simultaneously.

**Why it matters**: Can limit concurrency in Python. Rust doesn't have this limitation.

**How we handle it**: Our Rust code runs without GIL contention. The pool handles synchronization.

**Key point**: You don't need to worry about GIL - just know deadpool manages it.

---

## J

### JSONB

**What**: PostgreSQL's efficient JSON storage format. "B" stands for "binary".

**Why it matters**: FraiseQL uses JSONB for flexible data schemas. More efficient than JSON.

**Difference from JSON**:
- JSON: Human-readable but slower to query
- JSONB: Binary format, indexed, faster queries (↓ 20% query time)

**Example in SQL**:
```sql
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    profile JSONB  -- Store flexible data
);

-- Query JSONB fields
SELECT * FROM users WHERE profile->>'role' = 'admin';
```

---

## M

### Macro

**What**: Rust code that generates other Rust code at compile time.

**Common ones you'll see**:
- `println!()` - print debug output
- `dbg!()` - print variable and its value
- `assert!()` - test assertion
- `vec![]` - create a vector
- `todo!()` - placeholder (compile error if left in code)

**Why it matters**: Macros are powerful but can be confusing. End with `!()`.

---

### Mutex

**What**: "Mutual Exclusion" - a lock that ensures only one piece of code accesses data at a time.

**When you see it**: `Arc<Mutex<T>>` - shared, protected data.

**Why it matters**: Prevents data races in concurrent code.

**Example**:
```rust
use std::sync::{Arc, Mutex};

let counter = Arc::new(Mutex::new(0));  // Shared, locked counter

// To modify:
let mut count = counter.lock().unwrap();  // Get exclusive access
*count += 1;  // Modify
// Lock automatically released when `count` goes out of scope
```

---

## P

### Pattern Matching

**What**: Rust's powerful syntax for handling different cases.

**Examples**:
```rust
// Match on Result
match result {
    Ok(value) => println!("Success: {}", value),
    Err(e) => println!("Error: {}", e),
}

// Match on Option
match maybe_user {
    Some(user) => println!("User: {}", user.name),
    None => println!("No user found"),
}
```

**Why it matters**: Forces you to handle all cases (prevents unhandled errors).

---

### PyO3

**What**: Rust library for creating Python modules in Rust.

**What it does**:
- Defines Rust functions that Python can call
- Converts types between Python and Rust
- Handles errors at FFI boundary

**Reference**: https://pyo3.rs/

---

### PyO3-asyncio

**What**: Bridge between PyO3 and async Rust code.

**Problem it solves**: PyO3 alone is synchronous. This library lets us:
- Return Futures from Rust to Python
- Python can `.await` them

**Key function**:
```rust
pyo3_asyncio::tokio::future_into_py(py, async { ... })
```

---

## R

### Result<T, E>

**What**: Enum representing either success (Ok) or failure (Err).

**Why it matters**: Rust's way of handling errors without exceptions.

**Example**:
```rust
fn divide(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 {
        Err("Division by zero".to_string())
    } else {
        Ok(a / b)
    }
}

// Using it:
match divide(10, 2) {
    Ok(result) => println!("Result: {}", result),
    Err(e) => println!("Error: {}", e),
}

// Or with ? operator:
let result = divide(10, 2)?;  // Propagates error if it occurs
```

---

### Rust Edition

**What**: Version of Rust language syntax and features.

**Current**: 2021 (what FraiseQL uses)

**What changed between editions**:
- 2015: Original Rust
- 2018: Simplified module system, better async
- 2021: Better error messages, improved async

**You'll see**: `edition = "2021"` in `Cargo.toml`

---

## S

### Schema

**What**: Description of database table structure (columns, types, constraints).

**Example**:
```sql
CREATE TABLE users (
    id SERIAL PRIMARY KEY,      -- column: name, type, constraint
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    created_at TIMESTAMP DEFAULT NOW()
);
```

**In FraiseQL**: Phase 1 creates a schema registry that bridges Python and Rust.

---

### Slice

**What**: View into a portion of a collection (String, Vec, array).

**Why it matters**: Used throughout Rust code for efficiency (no copying).

**Example**:
```rust
let v = vec![1, 2, 3, 4, 5];
let slice = &v[1..4];  // References elements 1, 2, 3 (doesn't copy)
println!("{:?}", slice);  // [2, 3, 4]
```

---

### String vs &str

**What**: Two different string types in Rust.

**Difference**:
- `String` - Owned, mutable, allocated on heap
- `&str` - Borrowed, immutable, reference to data

**When to use**:
```rust
fn greet(name: &str) {  // Use &str for parameters
    println!("Hello, {}", name);
}

let greeting = String::from("Hello");  // Use String when you need to own it
greet(&greeting);  // Pass as &str
```

---

### Struct

**What**: Rust's way of grouping related data (like a class without methods).

**Example**:
```rust
struct User {
    id: i32,
    name: String,
    email: String,
}

// Create instance
let user = User {
    id: 1,
    name: "Alice".to_string(),
    email: "alice@example.com".to_string(),
};

// Access fields
println!("{}", user.name);
```

---

## T

### TDD (Test-Driven Development)

**What**: Write tests BEFORE writing code.

**Flow**:
1. Write test (it fails - "RED")
2. Write code to make test pass ("GREEN")
3. Refactor code to be clean ("REFACTOR")
4. Final test verification ("QA")

**Why we use it**: Tests define requirements clearly before coding.

---

### Tokio

**What**: Async runtime for Rust - manages async tasks and scheduling.

**What it does**:
- Runs multiple async tasks concurrently
- Handles threads in background
- Manages the event loop

**You'll see**: `#[tokio::main]` attribute and `tokio::spawn()` calls.

**Reference**: https://tokio.rs/

---

### Trait

**What**: Like an interface - defines methods that types must implement.

**Example**:
```rust
trait Animal {
    fn speak(&self) -> String;
}

impl Animal for Dog {
    fn speak(&self) -> String {
        "Woof!".to_string()
    }
}
```

**Common traits**:
- `Clone` - Make a copy
- `Debug` - Print for debugging
- `Iterator` - Loop over items

---

## U

### Unwrap()

**What**: Extracts a value from Result or Option, panics if not present.

**Dangerous**: Using `.unwrap()` can crash your program!

**Example**:
```rust
let x: Result<i32, String> = Ok(5);
let value = x.unwrap();  // Gets 5

let y: Result<i32, String> = Err("problem".to_string());
let value = y.unwrap();  // PANICS! - program crashes
```

**Better alternatives**:
```rust
// Use ? operator (propagates error)
let value = y?;

// Use match (handles both cases)
match y {
    Ok(v) => println!("Got: {}", v),
    Err(e) => println!("Error: {}", e),
}

// Use unwrap_or (provide default)
let value = y.unwrap_or(0);  // Returns 0 if error
```

---

## V

### Visibility (pub/private)

**What**: Controls whether code is accessible from outside a module.

**Rules**:
```rust
struct MyStruct { ... }      // Private - only visible in this module
pub struct MyStruct { ... }  // Public - visible everywhere

fn helper() { ... }          // Private
pub fn helper() { ... }      // Public

pub(crate) fn internal() {}  // Visible within crate
```

---

## W

### Where Clauses (SQL)

**What**: Filters database queries to specific rows.

**Example**:
```sql
SELECT * FROM users WHERE age > 18 AND role = 'admin';
```

**In FraiseQL**: Phase 2 implements WHERE clause building in Rust (converts from GraphQL filters to SQL).

---

## Z

### Zero-Copy

**What**: Passing data between systems without making copies.

**Why it matters**: Saves memory and CPU time for large result sets.

**Example**:
```
PostgreSQL Result (on disk)
  ↓
Rust reads into buffer (1 copy)
  ↓
Rust converts to JSON (in same buffer)
  ↓
Sends to HTTP (same buffer - NO COPY!)
  ↓
HTTP Response
```

**Traditional approach** (3+ copies):
```
PostgreSQL
  ↓ Copy 1: Into Python
  ↓ Copy 2: Transform in Python
  ↓ Copy 3: Into response
  ↓ HTTP
```

---

## Quick Reference by Phase

### Phase 0
- **Clippy** - Linter for Rust code quality
- **Macro** - Code generation (`todo!`, `println!`)
- **Rustfmt** - Code formatter

### Phase 1
- **Arc** - Shared ownership for connection pool
- **Mutex** - Lock for shared state
- **Tokio** - Async runtime
- **Deadpool** - Connection pool
- **Schema** - Table structure registry

### Phase 2
- **Where Clauses** - SQL filtering
- **Pattern Matching** - Handling results
- **Result<T, E>** - Error handling

### Phase 3
- **Zero-Copy** - Efficient streaming
- **JSONB** - JSON storage in PostgreSQL

### Phase 4
- **FFI** - Python-Rust boundary
- **PyO3** - Python module creation
- **PyO3-asyncio** - Async bridge

### Phase 5
- **Feature Flags** - Conditional compilation

---

## Common Abbreviations

| Abbreviation | Meaning |
|--------------|---------|
| FFI | Foreign Function Interface |
| GIL | Global Interpreter Lock |
| JSONB | JSON Binary |
| TDD | Test-Driven Development |
| ORM | Object-Relational Mapping |
| OID | Object ID (PostgreSQL type identifier) |
| CRUD | Create, Read, Update, Delete |
| REST | Representational State Transfer |
| SQL | Structured Query Language |
| CLI | Command-Line Interface |

---

## External Resources

### Rust
- **Official Book**: https://doc.rust-lang.org/book/
- **Rust by Example**: https://doc.rust-lang.org/rust-by-example/
- **Clippy Docs**: https://docs.rs/clippy/

### Async Rust
- **Tokio Tutorial**: https://tokio.rs/tokio/tutorial
- **Async Rust**: https://rust-lang.github.io/async-book/

### PostgreSQL
- **Official Docs**: https://www.postgresql.org/docs/
- **JSONB Guide**: https://www.postgresql.org/docs/current/datatype-json.html

### Libraries
- **PyO3**: https://pyo3.rs/
- **Deadpool**: https://docs.rs/deadpool-postgres/
- **Tokio-postgres**: https://docs.rs/tokio-postgres/

---

**Next**: When you encounter unfamiliar terms during implementation, check here first!
