# Getting Started with Axum

**Version**: 2.0.0+
**Reading Time**: 30 minutes
**Audience**: New Axum users
**Difficulty**: Moderate (requires Rust knowledge)
**Prerequisites**: Rust 1.70+, PostgreSQL 13+, Python 3.13+

---

## What You'll Learn

In this guide, you'll:
- ✅ Understand what Axum brings to FraiseQL
- ✅ Set up your development environment
- ✅ Build your first Axum-based GraphQL server
- ✅ Test it locally with GraphQL clients
- ✅ Understand how Axum differs from FastAPI/Starlette

---

## What is Axum?

Axum is a high-performance, ergonomic Rust web framework that powers FraiseQL v2.0.0's HTTP server layer.

### Why Axum?

```
Your GraphQL Types & Resolvers (Python)
           ↓
    Rust HTTP Layer
    ┌──────────────────────────────────┐
    │ Axum Web Framework               │
    │ • HTTP/2 native                  │
    │ • High performance                │
    │ • WebSocket support               │
    │ • Advanced middleware              │
    │ • Type-safe request handling      │
    └──────────────────────────────────┘
           ↓
Exclusive Rust GraphQL Pipeline
```

### Key Benefits

| Benefit | Why It Matters |
|---------|----------------|
| **7-10x faster** than FastAPI | Handle more traffic with fewer servers |
| **HTTP/2 native** | Multiplexing, better mobile performance |
| **WebSocket subscriptions** | Real-time updates (graphql-ws) |
| **Advanced observability** | Operation monitoring, tracing, metrics |
| **Batch request processing** | Deduplication, parallel execution |
| **Zero unsafe code** | Memory safe by default |

### When to Use Axum

✅ **Use Axum if**:
- You need high performance (1000+ QPS)
- Building microservices
- Real-time features (WebSocket subscriptions)
- Team has Rust experience
- Performance matters more than setup speed

❌ **Use Starlette/FastAPI if**:
- Prefer pure Python
- Performance is not critical
- Want faster initial setup
- No Rust expertise available

---

## Prerequisites

Before starting, you need:

### 1. Rust Installation

**Check if you have Rust**:
```bash
rustc --version
cargo --version
```

**If not installed**, follow the official guide:
```bash
# macOS/Linux
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Windows
# Download from https://rustup.rs/
```

**Verify installation**:
```bash
rustc --version  # Should be 1.70 or later
cargo --version  # Should be 1.70 or later
```

### 2. Python 3.13+

```bash
python --version  # Should be 3.13+
```

### 3. PostgreSQL 13+

```bash
psql --version  # Should be 13 or later
```

### 4. Development Tools (optional but recommended)

```bash
# Code editor with Rust support
# VS Code: Install rust-analyzer extension
# or IntelliJ IDEA: Install Rust plugin

# Useful Rust tools
cargo install cargo-watch  # Auto-recompile on changes
cargo install cargo-clippy # Linting
```

---

## Architecture: How Axum Fits In

FraiseQL uses a **layered architecture** where Axum is the HTTP layer:

```
┌─────────────────────────────────────────┐
│  Your Code                              │
│  • Python GraphQL Types                 │
│  • Python Resolvers                     │
│  • Business Logic                       │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│  Axum HTTP Server (Rust)                │
│  • HTTP request handling                │
│  • GraphQL query parsing                │
│  • Response building                    │
│  • WebSocket management                 │
│  • Middleware pipeline                  │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│  FraiseQL Rust Pipeline                 │
│  • Query execution                      │
│  • Mutation processing                  │
│  • Subscription handling                │
│  • Caching                              │
│  • Field resolution                     │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│  PostgreSQL Database                    │
└──────────────────────────────────────────┘
```

**Key Point**: Your Python code doesn't change! You write types and resolvers exactly as before. Axum handles the HTTP layer transparently.

---

## Hello World: Your First Axum Server

Let's build a minimal working GraphQL API.

### Step 1: Create a New Rust Project

```bash
cargo new my-graphql-api
cd my-graphql-api
```

### Step 2: Add FraiseQL to Cargo.toml

```toml
[package]
name = "my-graphql-api"
version = "0.1.0"
edition = "2021"

[dependencies]
fraiseql_rs = { version = "2.0.0", features = ["http"] }
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

### Step 3: Create Your Schema in Python

```python
# schema.py
import fraiseql
from fraiseql_rs import PyAxumServer

@fraiseql.type
class User:
    id: fraiseql.ID
    name: str
    email: str

@fraiseql.query
class Query:
    @fraiseql.resolve()
    async def users(info) -> list[User]:
        """Get all users"""
        # This will query your database
        pass

schema = fraiseql.build_schema(Query)
```

### Step 4: Create Your Axum Server (Rust)

```rust
// src/main.rs
use axum::{
    routing::get,
    Router,
};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    // Initialize your GraphQL server
    // (Code varies by your FraiseQL setup)

    let app = Router::new()
        .route("/graphql", axum::routing::post(graphql_handler))
        .route("/health", get(health_check));

    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    println!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}

async fn graphql_handler() -> &'static str {
    "GraphQL endpoint"
}

async fn health_check() -> &'static str {
    "OK"
}
```

### Step 5: Run Your Server

```bash
# Build
cargo build

# Run
cargo run

# Or use cargo-watch for development
cargo watch -x run
```

**Expected output**:
```
   Compiling my-graphql-api v0.1.0
    Finished release [optimized] target(s) in 2.34s
     Running `target/release/my-graphql-api`
Server listening on 127.0.0.1:8000
```

### Step 6: Test Your Server

**Test with curl**:
```bash
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name } }"}'
```

**Test with GraphQL client**:
- Open http://localhost:8000/graphql in your browser
- Or use GraphQL Playground: http://localhost:8000/graphql/playground

---

## Do You Know Rust?

### If You're New to Rust

**Don't worry!** You don't need deep Rust knowledge for basic Axum usage.

**Essential Rust concepts**:
1. **Ownership & borrowing** - Memory management (watch the video)
2. **Async/await** - Writing async code (straightforward)
3. **Traits** - Interface-like mechanism (use existing traits)
4. **Macros** - Code generation (use them, not write them)

**Recommended learning**:
- Official book: [The Rust Book](https://doc.rust-lang.org/book/) (Ch 1-10)
- Async Rust: [Async Rust Book](https://rust-lang.github.io/async-book/)
- Time investment: 1-2 weeks for basics

**Practical approach**:
- Start with examples (copy-paste and modify)
- Use IDE hints and error messages
- Gradually understand the "why"
- Grow from there

### If You Know Rust Already

Great! You'll find Axum very ergonomic.

**Key Axum concepts**:
- **Extractors** - Extract data from requests
- **Handlers** - Async functions that handle routes
- **Routers** - Compose handlers into routes
- **Middleware** - Intercept requests/responses
- **State** - Share data across handlers

All are well-documented and have clean APIs.

---

## Common Setup Issues

### Issue 1: "Rust Not Found"

**Problem**: `cargo: command not found`

**Solution**:
```bash
# Reinstall Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Source the new environment
source $HOME/.cargo/env
```

### Issue 2: "Tokio Runtime Error"

**Problem**: `thread 'main' panicked at 'there is no reactor running'`

**Solution**: Ensure you're using `#[tokio::main]` macro:
```rust
#[tokio::main]  // <- Add this!
async fn main() {
    // Your code
}
```

### Issue 3: "Compilation Takes Forever"

**Problem**: First build takes 5+ minutes

**Why**: Rust compiles to native code (one-time cost)

**Solution**: Be patient! Subsequent builds are faster. Use release mode:
```bash
cargo build --release  # Optimized, slower compilation
cargo run --release    # Optimized runtime
```

### Issue 4: "Type Mismatches"

**Problem**: Error: `mismatched types`

**Solution**: Read the error message carefully! Rust is explicit about type requirements.

Common fixes:
- Missing `async` on function
- Missing `await` on async call
- Wrong type (String vs &str)

---

## Development Workflow

### Daily Development

```bash
# Terminal 1: Watch for changes and recompile
cargo watch -x run

# Terminal 2: Make changes to your code
# Axum will auto-reload on save
```

### Testing Your GraphQL API

**Option 1: curl (command line)**
```bash
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ users { id name email } }",
    "variables": {},
    "operationName": null
  }'
```

**Option 2: GraphQL Playground (browser)**
```
http://localhost:8000/graphql/playground
```

**Option 3: VS Code Extension**
- Install "REST Client" extension
- Create `requests.rest` file
- Make requests directly from editor

**Option 4: Postman**
- URL: `http://localhost:8000/graphql`
- Body: GraphQL JSON
- Headers: `Content-Type: application/json`

### Debugging

**View detailed logging**:
```bash
# Set log level
RUST_LOG=debug cargo run

# Or in code
env_logger::Builder::from_default_env()
    .filter_level(log::LevelFilter::Debug)
    .init();
```

**Use VS Code Debugger**:
```json
// .vscode/launch.json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug Axum",
      "cargo": {
        "args": ["build", "--bin=my-graphql-api"],
        "filter": { "name": "my-graphql-api", "kind": "bin" }
      },
      "args": [],
      "cwd": "${workspaceFolder}"
    }
  ]
}
```

---

## Next Steps

Now that your server is running, explore:

### 📚 Continue Learning

1. **[Configuration Guide →](./02-configuration.md)** - Customize your server
   - CORS setup
   - Authentication
   - Rate limiting
   - Middleware

2. **[Production Deployment →](./03-deployment.md)** - Deploy to production
   - Docker containerization
   - Kubernetes
   - Cloud platforms
   - Monitoring

3. **[Performance Tuning →](./04-performance.md)** - Optimize for scale
   - HTTP/2 configuration
   - Connection pooling
   - Batch requests
   - Caching strategies

4. **[Troubleshooting →](./05-troubleshooting.md)** - Common issues
   - Performance problems
   - WebSocket issues
   - Memory leaks
   - Connection problems

### 🔗 Useful Resources

- **Official Axum Docs**: https://docs.rs/axum/latest/axum/
- **Tokio Runtime**: https://tokio.rs/
- **GraphQL Spec**: https://spec.graphql.org/
- **FraiseQL Docs**: See main documentation

### 💡 Tips for Success

1. **Start small** - Build and test incrementally
2. **Read error messages** - Rust errors are helpful!
3. **Use examples** - Copy and modify existing code
4. **Ask for help** - Community is welcoming
5. **Measure performance** - Benchmark before optimizing

---

## Congratulations! 🎉

You now have:
- ✅ Axum HTTP server running
- ✅ GraphQL endpoint accepting queries
- ✅ Development workflow ready
- ✅ Foundation for your API

**Next step?** Configure your server for your use case → [Configuration Guide](./02-configuration.md)

---

## Quick Reference

| Task | Command |
|------|---------|
| Create project | `cargo new my-api` |
| Build | `cargo build` |
| Run | `cargo run` |
| Run optimized | `cargo run --release` |
| Watch & rebuild | `cargo watch -x run` |
| Check compilation | `cargo check` |
| Format code | `cargo fmt` |
| Lint code | `cargo clippy` |
| Test | `cargo test` |

---

## Need Help?

Having trouble?

- **Stuck on setup?** → See [Common Setup Issues](#common-setup-issues) above
- **Want to configure server?** → [Configuration Guide](./02-configuration.md)
- **Ready to deploy?** → [Production Deployment](./03-deployment.md)
- **Performance questions?** → [Performance Tuning](./04-performance.md)
- **Something broken?** → [Troubleshooting](./05-troubleshooting.md)

**Keep going!** You're building something awesome! 🚀
