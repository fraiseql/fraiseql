# Junior Engineer's Survival Guide: Common Mistakes & Debugging

**Document**: Common pitfalls for first-time implementers + debugging strategies
**Created**: 2025-12-18
**When to use**: When something breaks or you're confused
**Goal**: Get unstuck in 5-10 minutes

---

## Before You Start

### Mental Shifts from Python → Rust

1. **"Compiler errors are helpful"**
   - Python: Runtime errors (program crashes)
   - Rust: Compile-time errors (caught before running)
   - **Mindset**: "Compiler is your friend catching bugs early"

2. **"The borrow checker is strict for a reason"**
   - Python: You can reference variables anywhere
   - Rust: Ownership matters (prevents memory corruption)
   - **Mindset**: "Learn why, then it clicks"

3. **"Async code requires .await everywhere"**
   - Python: Async happens automatically in many frameworks
   - Rust: You must explicitly `.await` futures
   - **Mindset**: "Compiler will tell you if you forgot"

---

## Phase 0: Common Mistakes

### Phase 0.1: Clippy

**Mistake 1: Ignoring Clippy warnings**

```rust
// ❌ BAD - Clippy warns about this
let x = String::from("hello");
let s = format!("{}", x);  // Unnecessary formatting

// ✅ GOOD
let x = String::from("hello");
let s = x;  // Use directly or
let s = x.clone();  // Explicit if you need a copy
```

**Fix**: Read the warning, it suggests the exact fix.

---

**Mistake 2: Using todo!() or unimplemented!()**

```rust
// ❌ WRONG - Will compile error with our Clippy config
fn fetch_user(id: i32) -> User {
    todo!()  // Compiler ERROR: todo!() macro denied
}

// ✅ CORRECT - Use placeholder that compiles
fn fetch_user(id: i32) -> User {
    panic!("Not implemented yet")  // Fails at runtime, not compile time
    // Or return a default:
    // User { id: 0, name: String::new() }
}
```

**How to recognize**: Compilation stops with "error: todo!() macro used".

---

**Mistake 3: Debug macros in production code**

```rust
// ❌ WRONG - Clippy warns
fn process_user(user: User) {
    dbg!(&user);  // Clippy: "debug macro"
    println!("Processing {}", user.name);  // Clippy: "println! macro"
    process(user);
}

// ✅ CORRECT - Use structured logging or tests
#[cfg(test)]  // Only in tests
mod tests {
    use super::*;
    #[test]
    fn test_process() {
        let user = User::new();
        dbg!(&user);  // OK in tests
        assert!(valid_user(&user));
    }
}
```

**How to recognize**: Clippy says "debug macro used".

---

### Phase 0.2: Tests

**Mistake 1: Tests that depend on execution order**

```rust
// ❌ WRONG - Tests run in parallel, this fails randomly
#[test]
fn test_create_user() {
    let pool = GLOBAL_POOL.get();  // Shared state!
    pool.execute("INSERT INTO users ...");
}

#[test]
fn test_count_users() {
    let pool = GLOBAL_POOL.get();
    let count = pool.execute("SELECT COUNT(*)...");
    assert_eq!(count, 1);  // Fails if test_create_user runs first!
}

// ✅ CORRECT - Each test gets its own database
#[test]
fn test_create_user() {
    let pool = TestDatabase::new();  // Fresh DB
    pool.execute("INSERT INTO users ...");
    assert_eq!(pool.count_users(), 1);
}

#[test]
fn test_other() {
    let pool = TestDatabase::new();  // Different fresh DB
    assert_eq!(pool.count_users(), 0);
}
```

**How to fix**: Use separate databases per test (that's what TestDatabase helper does).

---

**Mistake 2: Forgetting the `async` keyword**

```rust
// ❌ WRONG - async function but no async keyword
#[test]
fn test_async_query() {
    let result = fetch_user(1);  // Returns a Future, not a User!
    assert_eq!(result.name, "Alice");  // Can't access .name on Future
}

// ✅ CORRECT
#[tokio::test]  // Tokio test, not normal test
async fn test_async_query() {
    let result = fetch_user(1).await;  // NOW it waits and returns User
    assert_eq!(result.name, "Alice");
}
```

**How to recognize**: Compiler error: "no field `name` on type `impl Future`"

---

### Phase 0.3: Benchmarks

**Mistake 1: Benchmarks affected by other processes**

```rust
// ❌ RISKY - Results vary wildly
#[bench]
fn bench_query(b: &mut Bencher) {
    b.iter(|| {
        database.execute("SELECT * FROM users LIMIT 1")
    });
}

// ✅ BETTER - Control variables
#[bench]
fn bench_query(b: &mut Bencher) {
    let pool = setup_test_pool();  // Consistent setup

    b.iter(|| {
        pool.get_connection()  // Test just the specific thing
    });
}
```

**How to recognize**: Benchmark times vary wildly (10ms, 50ms, 15ms, 100ms)

---

## Phase 1: Common Mistakes

### Connection Pool

**Mistake 1: Forgetting Arc for shared pool**

```rust
// ❌ WRONG - Can't share pool across requests
pub struct ConnectionPool {
    pool: deadpool_postgres::Pool,  // Not wrapped in Arc
}

impl ConnectionPool {
    pub fn get_connection(&self) -> Connection {
        // ERROR: Can't clone pool without Arc!
    }
}

// ✅ CORRECT - Arc allows sharing
use std::sync::Arc;

pub struct ConnectionPool {
    pool: Arc<deadpool_postgres::Pool>,  // Wrapped in Arc
}

impl ConnectionPool {
    pub fn get_connection(&self) -> Connection {
        self.pool.get().await  // ✅ Works!
    }
}
```

**How to fix**: Wrap in `Arc::new()`.

---

**Mistake 2: Blocking in async context**

```rust
// ❌ WRONG - Blocks the entire tokio runtime!
async fn fetch_user(id: i32) -> User {
    std::thread::sleep(Duration::from_secs(1));  // BLOCKS everyone!
    database.query(id).await
}

// ✅ CORRECT - Use async sleep
async fn fetch_user(id: i32) -> User {
    tokio::time::sleep(Duration::from_secs(1)).await;  // Only blocks this task
    database.query(id).await
}
```

**Impact**: One blocking call can freeze the entire server. This is serious!

**How to recognize**: Server becomes unresponsive after a few requests.

---

**Mistake 3: Not awaiting async functions**

```rust
// ❌ WRONG - Forgot .await
async fn get_all_users() -> Vec<User> {
    let users = database.query("SELECT * FROM users");  // Forgot .await!
    // users is now a Future, not Vec<User>
    users  // Return Future instead of Vec
}

// ✅ CORRECT
async fn get_all_users() -> Vec<User> {
    let users = database.query("SELECT * FROM users").await;  // Got it!
    users
}
```

**How to recognize**: Compiler error: "expected Vec<User>, found impl Future"

---

**Mistake 4: Pool exhaustion**

```rust
// ❌ WRONG - Holds connections without releasing
pub async fn process_all_users() -> Vec<Result<()>> {
    let mut results = vec![];

    for id in 1..=1000 {
        let conn = pool.get().await;  // Gets a connection
        // Never releases it!
        results.push(do_something(&conn).await);
    }
    // After 10 requests, pool exhausted (max_size=10)
}

// ✅ CORRECT - Let scope manage connection lifetime
pub async fn process_all_users() -> Vec<Result<()>> {
    let mut results = vec![];

    for id in 1..=1000 {
        let result = {
            let conn = pool.get().await;  // Gets connection
            do_something(&conn).await  // Use it
        };  // Scope ends, connection released back to pool
        results.push(result);
    }
}
```

**How to recognize**: Deadlocks or timeouts after ~10 concurrent requests.

---

## Phase 2-3: Common Mistakes

### WHERE Clause Building

**Mistake 1: SQL injection vulnerability**

```rust
// ❌ CRITICALLY DANGEROUS - SQL injection!
fn build_query(user_id: i32, search: String) -> String {
    // User could pass search = "'; DROP TABLE users; --"
    format!("SELECT * FROM users WHERE id = {} AND name LIKE '%{}%'", user_id, search)
}

// ✅ SAFE - Parameterized queries
fn build_query(user_id: i32, search: String) -> (String, Vec<&str>) {
    let query = "SELECT * FROM users WHERE id = $1 AND name LIKE $2";
    let params = vec![&user_id.to_string(), &format!("%{}%", search)];
    (query.to_string(), params)
}
```

**Why**: Never concatenate user input into SQL strings!

---

**Mistake 2: Type conversion errors**

```rust
// ❌ WRONG - Converting wrong type
fn parse_filter(filter: PyObject) -> Result<Filter> {
    // Assumes filter is a dict
    let value: i32 = filter.extract()?;  // ERROR if not i32!
}

// ✅ CORRECT - Handle all types
fn parse_filter(filter: PyObject) -> Result<Filter> {
    if let Ok(s) = filter.extract::<String>() {
        return Ok(Filter::String(s));
    }
    if let Ok(i) = filter.extract::<i32>() {
        return Ok(Filter::Int(i));
    }
    Err("Unknown type")
}
```

**How to fix**: Type conversion from Python needs to handle multiple types.

---

## Phase 4: Common Mistakes

### Python-Rust Integration

**Mistake 1: Forgetting .into_py() conversion**

```rust
#[pyfunction]
fn get_user(py: Python, id: i32) -> PyResult<PyObject> {
    let user = User { id, name: "Alice".to_string() };

    // ❌ WRONG - Can't return Rust struct directly
    // Ok(user)  // ERROR: expected PyObject

    // ✅ CORRECT - Convert to Python
    Ok(user_to_python(py, &user)?)
}
```

**How to fix**: Use `.into_py(py)` or a conversion function.

---

**Mistake 2: GIL contention in loops**

```rust
// ❌ SLOW - Acquires GIL in each iteration
#[pyfunction]
fn process_many(py: Python, items: Vec<PyObject>) -> PyResult<Vec<PyObject>> {
    let mut results = vec![];
    for item in items {
        let result = Python::with_gil(|py| {  // Acquires GIL!
            // Process item
            Ok(item)
        })?;
        results.push(result);
    }
    Ok(results)
}

// ✅ BETTER - Acquire GIL once
#[pyfunction]
fn process_many(py: Python, items: Vec<PyObject>) -> PyResult<Vec<PyObject>> {
    Python::with_gil(|py| {  // Single GIL acquisition
        let mut results = vec![];
        for item in items {
            results.push(item);  // Process items
        }
        Ok(results)
    })
}
```

---

## Reading Rust Compiler Errors

### Error: "error[E0382]: use of moved value"

```
error[E0382]: use of moved value: `s`
 --> src/main.rs:5:20
  |
3 |     let s = String::from("hello");
  |         - binding `s` is moved into the following function call
4 |     println!("{}", s);
5 |     println!("{}", s);  // error: s was moved
```

**What it means**: You used a value twice, but it was moved (ownership transferred).

**Fix**:

```rust
let s = String::from("hello");
println!("{}", &s);  // Borrow instead of move
println!("{}", &s);  // ✅ Works now
```

---

### Error: "error[E0503]: cannot borrow as mutable because it's also borrowed as immutable"

```
error[E0503]: cannot borrow `x` as mutable because it's also borrowed as immutable
 --> src/main.rs:4:5
  |
3 |     let r = &x;
  |             -- immutable borrow occurs here
4 |     let rr = &mut x;
  |             ^^^^^^ mutable borrow not allowed while immutable borrow is active
```

**What it means**: You're trying to mutate something while someone else is reading it.

**Fix**: Drop the read reference before mutating:

```rust
let r = &x;
println!("{}", r);  // Use r
drop(r);  // Explicitly drop it
let rr = &mut x;  // ✅ Now OK
```

---

### Error: "error[E0597]: `x` does not live long enough"

```
error[E0597]: `x` does not live long enough
 --> src/main.rs:7:12
  |
5 | fn get_ref() -> &String {
  |                 ------- lifetime `'1` required to live as long as `'static`
6 |     let x = String::from("hello");
  |         - binding `x` is dropped at the end of the function
7 |     &x  // error: returns reference to x which is dropped
```

**What it means**: You're returning a reference to something that will be destroyed.

**Fix**: Return the owned value, not a reference:

```rust
fn get_string() -> String {  // Not &String
    let x = String::from("hello");
    x  // ✅ Ownership transferred
}
```

---

### Error: "error[E0308]: mismatched types"

```
error[E0308]: mismatched types
 --> src/main.rs:4:18
  |
4 |     let x: i32 = "hello";
  |            ---   ^^^^^^^ expected `i32`, found `&str`
```

**What it means**: Type mismatch. You promised one type but provided another.

**Fix**: Convert or fix the declaration:

```rust
let x: &str = "hello";  // ✅ Correct type
// or
let x: String = "hello".to_string();  // ✅ Convert
```

---

## Debugging Strategies

### Strategy 1: Use `dbg!()` in Tests (Only!)

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_filter() {
        let filter = parse_filter("age > 18").unwrap();
        dbg!(&filter);  // Prints the filter for inspection
        assert_eq!(filter.field, "age");
    }
}
```

**Output**:

```
[src/lib.rs:42] &filter = Filter {
    field: "age",
    operator: Greater,
    value: "18",
}
```

---

### Strategy 2: Read the Compiler Error Carefully

Rust compiler errors are verbose but specific. They tell you:

1. **What** went wrong (top line)
2. **Where** it happened (file and line)
3. **Context** (surrounding code)
4. **Suggestion** (how to fix it)

**Example**:

```
error: this expression will panic at runtime
 --> src/main.rs:123:15
  |
123 |     let x = vec[100].unwrap();
  |             ---------- index 100 is out of bounds for vec of length 50
  |
  = note: this will cause a panic if reached
  = help: consider checking the length first
```

**Action**: Follow the "help" suggestion!

---

### Strategy 3: Add Type Hints to Clarify

```rust
// ❌ Compiler confused
let result = database.query(id);  // What type is this?

// ✅ Explicit type
let result: Result<User, Error> = database.query(id);
// Now compiler can check if you're using it correctly
```

---

### Strategy 4: Break Into Smaller Functions

```rust
// ❌ Complex, hard to debug
fn process_users_complex(pool: Arc<Pool>) -> Result<Vec<User>> {
    let conns: Vec<_> = (1..10).map(|i| pool.get()).collect();
    // ... 20 more lines
}

// ✅ Testable, easier to debug
fn get_connections(pool: Arc<Pool>, count: usize) -> Vec<Connection> {
    (1..=count).map(|_| pool.get()).collect()
}

#[test]
fn test_get_connections() {
    let conns = get_connections(pool, 5);
    dbg!(&conns);  // Debug just this part
    assert_eq!(conns.len(), 5);
}
```

---

## When to Ask for Help

🟢 **You should try to solve (1-2 hours max)**:

- Compiler error you haven't seen before
- Test that's failing
- Small logic bug

🟡 **After 1-2 hours, ask for help**:

- Compiler error keeps appearing despite fixes
- Can't understand phase documentation
- Test infrastructure not working
- Async code deadlocking

🔴 **Ask immediately (something is very wrong)**:

- Code crashes with "panicked at"
- Pool exhaustion/deadlocks
- Memory usage growing infinitely
- Data corruption

---

## Quick Reference Checklist

Before asking for help, verify:

- [ ] Did I read the compiler error message twice?
- [ ] Did I check the phase's Troubleshooting section?
- [ ] Did I check GLOSSARY.md for unfamiliar terms?
- [ ] Did I try adding type hints?
- [ ] Did I try `cargo clean && cargo build`?
- [ ] Did I check if I forgot `.await` somewhere?
- [ ] Did I run the tests for this phase?
- [ ] Did I check the POC example code?

---

## Common "It's Not Working" Scenarios

### "My test passes locally but fails in CI"

**Cause**: Usually environment differences (database, timing).

**Debug**:

1. Check if test uses `TestDatabase` (isolated)
2. Check if test has race conditions (run locally 10x: `for i in {1..10}; do cargo test --lib test_name; done`)
3. Check if test is time-dependent (may be flaky)

---

### "I get compile errors but my code looks right"

**Cause**: Usually a subtle mistake.

**Debug**:

1. Copy-paste the error message into search
2. Check Rust documentation for the error code
3. Look at the "note" or "help" in the error
4. Try making a minimal example that reproduces it

---

### "Async code seems to hang"

**Cause**: Missing `.await` or blocking code in async context.

**Debug**:

```rust
// Add prints to see where it gets stuck
async fn my_function() {
    println!("1. Starting");
    let result = database.query().await;  // Make sure .await is here
    println!("2. Got result: {:?}", result);
}
```

---

### "Pool exhaustion errors"

**Cause**: Holding connections too long or forgetting to release them.

**Debug**:

- Verify each connection is released in a scope
- Check for infinite loops that keep getting connections
- Use `#[test] async fn` not `#[tokio::test]` for simpler tests

---

## Getting Unstuck: 10-Minute Process

1. **(1 min)** Read compiler error 2x slowly
2. **(2 min)** Search GLOSSARY.md for unfamiliar terms
3. **(3 min)** Check phase's Troubleshooting section
4. **(2 min)** Look at POC or earlier phase example
5. **(2 min)** Ask for code review / second pair of eyes

If still stuck: **It's time to ask for help. That's OK!**

---

**Remember**: Every Rust programmer has been stuck on the borrow checker. That's normal. It gets easier!
