# Prerequisites: Knowledge & Skill Requirements

**Document**: Skills and knowledge required before starting implementation
**Created**: 2025-12-18
**Duration**: 1-2 days of preparation (if starting from scratch)
**Critical**: YES - Mismatched skills will lead to frustration

---

## Quick Assessment

**Answer these questions to find your starting point:**

1. Have you written Rust code before?
   - ✅ Yes, I've built small projects → **START AT: Environment Setup**
   - ⚠️ I've done tutorials → **START AT: Rust Refresh Below**
   - ❌ Never touched Rust → **START AT: Learn Rust Basics (2-3 days)**

2. Do you understand `async/await`?
   - ✅ Yes, I use it regularly → Good!
   - ⚠️ I've seen it but not used it → **Learn Async Basics Below**
   - ❌ What's async? → **CRITICAL - Read Async Fundamentals (1 day)**

3. Are you comfortable with PostgreSQL?
   - ✅ I write SQL regularly → Good!
   - ⚠️ I know basic SELECT/INSERT → **Good enough**
   - ❌ What's PostgreSQL? → **Learn SQL Basics Below (1 day)**

---

## Rust Experience

### If You're New to Rust

**Time commitment**: 2-3 days to be ready

**What you MUST understand**:

1. **Ownership & Borrowing**
   - Variables have owners
   - You can borrow values (with `&`) without taking ownership
   - References are either mutable (`&mut`) or immutable (`&`)
   - Why? Because Rust prevents data races at compile time

   **Example**:
   ```rust
   let s = String::from("hello");      // s owns the string
   let r1 = &s;                         // r1 borrows s (immutable)
   let r2 = &s;                         // r2 also borrows s (OK - multiple readers)
   // Can't do: let r3 = &mut s;        // ERROR - can't borrow mutable + immutable
   ```

2. **Error Handling with Result**
   - Functions return `Result<T, E>` (success or failure)
   - Use `?` operator to propagate errors
   - Use pattern matching to handle results

   **Example**:
   ```rust
   fn get_user(id: i32) -> Result<User, Error> {
       let conn = database.connect()?;      // ? propagates error if connect fails
       conn.query("SELECT * FROM users WHERE id = $1", &[&id])
   }
   ```

3. **Traits** (interfaces)
   - Similar to interfaces in other languages
   - Define what methods a type must implement
   - Used extensively in Rust (Iterator, Clone, Debug, etc.)

4. **Pattern Matching**
   - `match` expressions are powerful
   - Used with Options and Results

   **Example**:
   ```rust
   match result {
       Ok(value) => println!("Success: {}", value),
       Err(e) => println!("Error: {}", e),
   }
   ```

**Resources** (1-2 hours each):
- [Rust Book - Ownership Chapter](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)
- [Rust Book - Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [Learn Rust with Rustlings (interactive exercises)](https://github.com/rust-lang/rustlings)

### If You Know Rust But Haven't Used Async

**Time commitment**: 1 day

**What you MUST understand**:

1. **Async/Await Syntax**
   - `async fn` returns a Future
   - `.await` waits for the Future to complete
   - Multiple futures can run concurrently (not in parallel, but interleaved)

   **Example**:
   ```rust
   async fn fetch_user(id: i32) -> User {
       // This function doesn't run immediately
       // It returns a Future that can be awaited later
       database.query("SELECT * FROM users WHERE id = $1", &[&id]).await
   }

   async fn main() {
       let user = fetch_user(1).await;  // Wait for the Future to complete
   }
   ```

2. **Concurrency with tokio**
   - `tokio::spawn(future)` runs multiple futures concurrently
   - Use `tokio::join!()` to wait for multiple futures
   - Connection pools handle the tokio runtime

   **Example**:
   ```rust
   async fn fetch_all_users() -> Vec<User> {
       let user1 = tokio::spawn(fetch_user(1));
       let user2 = tokio::spawn(fetch_user(2));
       let user3 = tokio::spawn(fetch_user(3));

       // All three run concurrently!
       let (u1, u2, u3) = tokio::join!(user1, user2, user3);
       vec![u1, u2, u3]
   }
   ```

3. **When Async Code Blocks**
   - Never call blocking code (like `std::thread::sleep`) in async context
   - Use async alternatives: `tokio::time::sleep()`
   - This is why `deadpool-postgres` is async (doesn't block the event loop)

**Resources** (1-2 hours):
- [Rust Book - Async Chapter](https://doc.rust-lang.org/book/ch17-00-async-await.html)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)

---

## PostgreSQL Knowledge

### If You're New to SQL

**Time commitment**: 1 day

**What you NEED to know**:

1. **Basic Queries**
   ```sql
   -- SELECT: get data
   SELECT * FROM users WHERE id = 1;
   SELECT id, name, email FROM users;

   -- INSERT: add data
   INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com');

   -- UPDATE: modify data
   UPDATE users SET email = 'new@example.com' WHERE id = 1;

   -- DELETE: remove data
   DELETE FROM users WHERE id = 1;
   ```

2. **Column Types**
   - `INT` / `BIGINT` - integers
   - `TEXT` / `VARCHAR` - strings
   - `BOOLEAN` - true/false
   - `TIMESTAMP` - dates and times
   - `JSON` / `JSONB` - JSON data (JSONB is better - indexed)
   - `SERIAL` - auto-incrementing integer

3. **Constraints & Keys**
   - `PRIMARY KEY` - unique identifier for each row
   - `FOREIGN KEY` - links to another table
   - `NOT NULL` - required column
   - `UNIQUE` - no duplicates

**Resources** (2-3 hours):
- [PostgreSQL Official Tutorial](https://www.postgresql.org/docs/current/tutorial.html)
- [SQL in 100 Seconds (YouTube)](https://www.youtube.com/watch?v=zsjvGqFqWBc)

### If You Know SQL

**Additional PostgreSQL concepts** (specific to this project):

1. **JSONB Type**
   - Stores JSON data efficiently
   - Indexed, faster than JSON
   - FraiseQL heavily uses JSONB for flexible schemas

   **Example**:
   ```sql
   CREATE TABLE users (
       id SERIAL PRIMARY KEY,
       name TEXT NOT NULL,
       data JSONB  -- Flexible schema
   );

   -- Query JSONB fields
   SELECT * FROM users WHERE data->>'role' = 'admin';
   ```

2. **Parameterized Queries**
   - **NEVER concatenate SQL strings** (SQL injection risk)
   - Use `$1`, `$2` placeholders
   - tokio-postgres handles this automatically

   ```rust
   // ✅ SAFE
   conn.execute("SELECT * FROM users WHERE id = $1", &[&user_id]).await

   // ❌ DANGEROUS - Never do this!
   let query = format!("SELECT * FROM users WHERE id = {}", user_id);
   ```

3. **Connection Pools**
   - Create connections once, reuse them
   - Connections are expensive (TCP connection to DB)
   - Pool manages max connections, timeout, etc.

---

## PyO3 Knowledge

### What You NEED to Know About PyO3

**Time commitment**: 2-3 hours (learning as you go)

**Concepts**:

1. **FFI (Foreign Function Interface)**
   - Allows Rust code to be called from Python
   - Requires type conversion at the boundary
   - Errors must convert to Python exceptions

2. **Basic PyO3 Pattern**
   ```rust
   use pyo3::prelude::*;

   #[pyfunction]  // Makes this callable from Python
   fn add(a: i32, b: i32) -> i32 {
       a + b
   }

   #[pymodule]  // Creates a Python module
   fn _fraiseql_rs(_py: Python, m: &PyModule) -> PyResult<()> {
       m.add_function(wrap_pyfunction!(add, m)?)?;
       Ok(())
   }
   ```

3. **Type Conversion**
   - `.extract::<Type>()` - Convert Python object to Rust
   - `.into_py(py)` - Convert Rust value to Python object

**You'll learn PyO3 by doing - the POC has excellent examples**

---

## Architecture Understanding

### What You Need to Know

1. **Why Rust?**
   - Performance (20-30% faster queries)
   - Memory safety (prevents entire classes of bugs)
   - Type safety (compile-time error checking)

2. **Why the Python-Rust Split?**
   - Python: User-friendly API, GraphQL layer, validation
   - Rust: Fast database operations, connection pooling, result streaming

3. **Data Flow**
   ```
   Python API Call
       ↓
   Validation & Schema Checking (Python)
       ↓
   PyO3 Call Boundary
       ↓
   Rust: Connection Pool + Query Execution
       ↓
   PostgreSQL
       ↓
   Results back to Rust
       ↓
   PyO3 Conversion to Python
       ↓
   Python Response Formatting
       ↓
   HTTP Response
   ```

---

## Honest Gaps & How to Handle Them

### "I don't understand Rust closures"

- **When you'll hit it**: Phase 2 (WHERE clause builder uses closures)
- **How to handle it**: Read the phase's "Troubleshooting" section first
- **Time to learn**: 1-2 hours with examples

### "I don't understand GIL contention"

- **When you'll hit it**: Phase 1 & 4 (Python-Rust integration)
- **How to handle it**: The PoC document explains it thoroughly
- **You don't need to**: Understand deeply - just know deadpool handles it

### "Async errors confuse me"

- **When you'll hit it**: Phase 1, Phase 4
- **How to handle it**: Use `.await` on all async calls; compiler will catch mistakes
- **Time to learn**: 2-3 hours with hands-on debugging

---

## Pre-Flight Checklist

Before starting, verify you can:

- [ ] **Rust**: `cargo new hello && cargo build && cargo run`
- [ ] **PostgreSQL**: `psql --version` and can create a test database
- [ ] **Git**: `git clone`, `git branch`, `git commit`
- [ ] **Terminal/CLI**: Can navigate directories, run commands
- [ ] **Python**: Understand basic async/await or willing to learn it
- [ ] **Debugging**: Can use `println!()` and `dbg!()` macros

---

## Recommended Preparation Path

### For Rust Beginners (Total: 3 days)

**Day 1** (6 hours):
- [ ] Rust Book Chapters 1-4 (Ownership)
- [ ] Rust Book Chapter 9 (Error Handling)
- [ ] Rustlings exercises (ownership + error handling)

**Day 2** (6 hours):
- [ ] SQL basics (PostgreSQL tutorial)
- [ ] Write 5 simple SELECT/INSERT queries manually
- [ ] Understand JSONB concept

**Day 3** (6 hours):
- [ ] Rust Book Chapter 17 (Async/Await)
- [ ] Tokio tutorial
- [ ] Write small async Rust program with basic operations

**Then**: Start at Phase 0.1 (Clippy)

### For Rust Developers Without Async (Total: 2 days)

**Day 1** (6 hours):
- [ ] Rust Book Chapter 17 (Async/Await)
- [ ] Tokio tutorial
- [ ] Hands-on: Write async functions with `.await`

**Day 2** (6 hours):
- [ ] SQL basics refresher
- [ ] JSONB concept
- [ ] Review PyO3 basics section above

**Then**: Start at Phase 0.1 (Clippy)

### For Experienced Rust + Async (Total: 1 day)

**Day 1** (6 hours):
- [ ] Read GLOSSARY.md (in this directory)
- [ ] Review Architecture section above
- [ ] Skim Phase 0.1-0.2 to understand structure

**Then**: Start at Phase 0.1 (Clippy)

---

## Red Flags: When to Ask For Help

🚨 **Stop and ask senior developer if**:

1. You don't understand the architecture diagram in README.md
2. You get a compile error you can't understand after reading the error message twice
3. Phase 0.2 test infrastructure confuses you
4. POC validation doesn't pass and you can't debug why
5. You're spending > 2 hours on a single "simple" task

✅ **These are normal**:

1. First Rust code compiling takes longer
2. Compiler errors are cryptic (embrace them - they prevent bugs!)
3. Async code feels weird at first
4. PyO3 type conversions look complex

---

## Getting Help

When stuck:

1. **Read the phase's Troubleshooting section** (most problems are documented)
2. **Check GLOSSARY.md** (terminology might be unfamiliar)
3. **Read compiler error messages carefully** (Rust compiler is actually helpful!)
4. **Check JUNIOR_GUIDE.md** (common mistakes section)
5. **Ask senior developer** - don't spend > 1 hour stuck

---

## Success Metrics

By the end of preparation, you should:

✅ Run `cargo build` without errors
✅ Write simple async Rust code
✅ Understand what `Arc<Mutex<T>>` means (shared, mutable state)
✅ Know the difference between `&s` and `&mut s`
✅ Understand why `.await` is needed
✅ Be comfortable reading PostgreSQL docs

---

**Next**: Read `ENVIRONMENT_SETUP.md` to install all required tools
