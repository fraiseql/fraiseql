# FraiseQL Docker Quick Start (Production - Pre-built Images)

**Get FraiseQL running in 30 seconds with zero Rust compilation!**

---

## The Fastest Way to Try FraiseQL

### Option 1: Single Example (Blog) - Minimal

```bash
docker compose -f docker/docker-compose.prod.yml up -d
```

That's it! Open your browser:
- **GraphQL IDE**: http://localhost:3000
- **Tutorial**: http://localhost:3001
- **Admin Dashboard**: http://localhost:3002

### Option 2: All Examples - Comprehensive

```bash
docker compose -f docker/docker-compose.prod-examples.yml up -d
```

Then explore:
- **Blog IDE**: http://localhost:3000 (simple)
- **E-Commerce IDE**: http://localhost:3100 (intermediate)
- **Streaming IDE**: http://localhost:3200 (advanced)
- **Tutorial**: http://localhost:3001 (guided learning)
- **Admin Dashboard**: http://localhost:3002 (debugging)

### Option 3: Using Make Commands (Easiest)

```bash
# Start single example
make prod-start

# Or start all examples
make prod-examples-start

# Check status
make prod-examples-status

# View logs
make prod-examples-logs

# Stop and cleanup
make prod-examples-clean
```

---

## What You're Running

### Pre-built Docker Images

No compilation required! All images are pre-built and ready to pull:

```
✅ fraiseql/server:latest       - GraphQL execution engine
✅ fraiseql/tutorial:latest     - Interactive tutorial
✅ fraiseql/dashboard:latest    - Admin dashboard & debugging
✅ postgres:16-alpine           - Database (PostgreSQL)
✅ graphql/graphql-playground   - Query IDE
```

**Total download**: ~800MB (compressed)
**Time to running**: 30-60 seconds

### Services Provided

| Service | Port | Purpose | Tech |
|---------|------|---------|------|
| GraphQL Server | 8000 | GraphQL execution | Rust |
| GraphQL IDE | 3000 | Query explorer | Web UI |
| Tutorial | 3001 | Learn FraiseQL | Node.js |
| Admin Dashboard | 3002 | Debug & monitor | Node.js |
| PostgreSQL | 5432 | Blog database | SQL |
| (+ E-Commerce DB) | 5433 | E-commerce data | SQL |
| (+ Streaming DB) | 5434 | Real-time events | SQL |

---

## Try Your First Query

### 1. Open GraphQL IDE

Navigate to: http://localhost:3000

### 2. Write a Query

```graphql
query GetAllUsers {
  users(limit: 10) {
    id
    name
    email
  }
}
```

### 3. Execute

Press the "Play" button → See results instantly!

### 4. Explore

Try these queries:
- Get all posts
- Get user with posts
- Filter by email
- Create new user (if mutations supported)

---

## Learn with the Tutorial

### Self-Guided Learning

1. Open http://localhost:3001
2. Start with **Chapter 1: What is FraiseQL?**
3. Progress through chapters at your own pace
4. Execute queries interactively
5. See compiled SQL in real-time

### Learning Path

```
30 minutes:  What is FraiseQL? How compilation works
15 minutes:  Your first query
15 minutes:  Relationships & JOINs
30 minutes:  E-Commerce example exploration
30 minutes:  Real-time & Streaming patterns
```

---

## Monitor with Admin Dashboard

### System Health

Navigate to: http://localhost:3002

### Pages Available

1. **Overview** - System health, uptime, request rate
2. **Schema Explorer** - Browse types, fields, relationships
3. **Query Debugger** - Execute & debug GraphQL queries
4. **Metrics** - Performance analysis, response times
5. **Logs** - System events and error tracking

### Features

- Real-time metrics
- Query complexity analysis
- SQL visualization
- Performance histograms
- Error tracking
- Log aggregation

---

## Database Access

### Direct Database Connection

```bash
# Blog database
psql -h localhost -p 5432 -U fraiseql -d blog_fraiseql

# E-Commerce database (if running prod-examples)
psql -h localhost -p 5433 -U fraiseql -d ecommerce_fraiseql

# Streaming database (if running prod-examples)
psql -h localhost -p 5434 -U fraiseql -d streaming_fraiseql
```

**Password**: `fraiseql_dev`

### Sample Queries

```sql
-- See all users
SELECT * FROM users;

-- See all posts
SELECT p.id, p.title, u.name FROM posts p JOIN users u ON p.author_id = u.id;

-- See products by category (if using ecommerce)
SELECT p.name, c.name FROM products p JOIN categories c ON p.category_id = c.id;
```

---

## Common Commands

### Start / Stop

```bash
# Start demo (single example)
make prod-start

# Start all examples
make prod-examples-start

# Stop
make prod-stop
make prod-examples-stop

# Clean (remove data)
make prod-clean
make prod-examples-clean
```

### Check Status

```bash
# View running containers
docker compose -f docker/docker-compose.prod-examples.yml ps

# Check health
make prod-examples-status

# View logs
make prod-examples-logs
```

### Development Commands

```bash
# Stop temporarily (keeps data)
docker compose -f docker/docker-compose.prod.yml down

# Restart
docker compose -f docker/docker-compose.prod.yml up -d

# Full cleanup (remove data)
docker compose -f docker/docker-compose.prod.yml down -v
```

---

## Troubleshooting

### Services not starting?

```bash
# Check logs
docker compose -f docker/docker-compose.prod.yml logs

# Check specific service
docker logs fraiseql-prod-server

# View all containers
docker ps -a
```

### Port already in use?

```bash
# Find what's using port 8000
lsof -i :8000

# Or use different port
# Edit docker-compose.prod.yml and change ports
```

### Out of memory?

```bash
# Check current usage
docker stats

# Docker Desktop: Settings → Resources → Memory (increase)

# Or run just one example instead of three
make prod-stop
make prod-start
```

### GraphQL endpoint not responding?

```bash
# Test directly
curl http://localhost:8000/health

# Wait a bit longer (startup takes 10-20 seconds)
sleep 5
curl http://localhost:8000/graphql

# Check server is running
docker ps | grep fraiseql-server
```

---

## Performance Expectations

### Startup Time

| Stack | First Time | Subsequent |
|-------|-----------|-----------|
| Single example | 30-45 seconds | 10-15 seconds |
| All 3 examples | 60-90 seconds | 20-30 seconds |

### Query Performance

| Query | Response Time |
|-------|----------------|
| Simple (get users) | 5-10ms |
| Moderate (with joins) | 15-30ms |
| Complex (aggregation) | 50-100ms |

### Resource Usage

| Stack | RAM | Disk | CPU |
|-------|-----|------|-----|
| Single example | ~600MB | ~300MB | 5-15% |
| All 3 examples | ~1.2GB | ~900MB | 10-20% |

---

## Example Complexity Levels

### Level 1: Blog (Beginner - 30 minutes)

```graphql
# Simple query
query {
  users {
    id
    name
    posts {
      title
    }
  }
}
```

**Concepts**: Lists, nested fields, basic relationships

### Level 2: E-Commerce (Intermediate - 1 hour)

```graphql
# Complex query with filtering
query {
  customer(id: 1) {
    name
    orders {
      totalPrice
      items {
        product {
          name
          price
        }
        quantity
      }
    }
  }
}
```

**Concepts**: Filtering, deep nesting, aggregation

### Level 3: Streaming (Advanced - 2+ hours)

```graphql
# Real-time subscription
subscription {
  onEvent(type: "user_action") {
    id
    timestamp
    data
  }
}
```

**Concepts**: Subscriptions, real-time data, events

---

## What's Different from Development?

### Development Mode

```bash
docker compose -f docker/docker-compose.demo.yml up -d
```

- Builds images locally (requires Rust compiler)
- Changes to code are reflected immediately (if you rebuild)
- Good for: Development, testing, modifying code

### Production Mode

```bash
docker compose -f docker/docker-compose.prod.yml up -d
```

- Uses pre-built images from Docker Hub
- No Rust installation needed
- Reproducible deployments
- Good for: Learning, demos, running examples

---

## Docker Hub Images

Pre-built images are automatically published to Docker Hub:

```
docker pull fraiseql/server:latest
docker pull fraiseql/tutorial:latest
docker pull fraiseql/dashboard:latest
```

**Built on every commit to main branch**
**Available at**: https://hub.docker.com/r/fraiseql

---

## System Requirements

### Minimum

- Docker Desktop / Docker Engine
- 4GB RAM available
- 2GB disk space
- Internet connection (for image pull)

### Recommended

- 8GB+ RAM
- 5GB+ disk space
- Stable internet
- Linux, macOS, or Windows 10+

### Check System Requirements

```bash
# Check Docker
docker --version
# Should be 20.10+

# Check memory
docker stats
# Should show 4GB+ available

# Check disk
df -h
# Should show 5GB+ free
```

---

## Getting Help

### Documentation

- **Docker Quick Start**: This file (you're reading it!)
- **Phase 4 Details**: `.docker-phase4-status.md` (examples guide)
- **Phase 5 Details**: `.docker-phase5-status.md` (CI/CD & deployment)
- **Architecture**: `.claude/CLAUDE.md` (design principles)

### Tutorial

- **Interactive**: http://localhost:3001 (start here!)
- **Web-based**: 6 chapters covering all concepts
- **Hands-on**: Execute queries in real-time

### Admin Dashboard

- **Debugging**: http://localhost:3002 (for troubleshooting)
- **Query executor**: Test GraphQL queries
- **Schema explorer**: Browse available types
- **Metrics**: Monitor performance

### Example Files

```
examples/
├── basic/              # Blog example (2 types)
│   ├── schema.json
│   ├── sql/setup.sql
│   └── queries/        # Sample queries
├── ecommerce/          # E-Commerce (5 types)
│   ├── schema.json
│   ├── sql/setup.sql
│   └── queries/        # 5 sample queries
└── streaming/          # Real-time (4 types + subscriptions)
    ├── schema.json
    ├── sql/setup.sql
    └── queries/        # 4 sample queries
```

---

## Next Steps

### 1. Explore (5 minutes)
- Open GraphQL IDE: http://localhost:3000
- Try sample queries
- Understand GraphQL basics

### 2. Learn (30 minutes)
- Go to tutorial: http://localhost:3001
- Complete all chapters
- Run hands-on queries

### 3. Experiment (1 hour)
- Use Admin Dashboard: http://localhost:3002
- Try different queries
- Explore schema
- Check performance metrics

### 4. Deep Dive (2+ hours)
- Switch to e-commerce example
- Explore streaming example
- Read documentation
- Understand compilation

---

## Want to Modify Code?

### If you want to modify FraiseQL itself:

```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Use development compose
docker compose -f docker/docker-compose.demo.yml up -d

# 3. Make changes to code
# 4. Rebuild
cargo build

# 5. Restart
docker compose up -d
```

### For production deployment:

Stick with `docker-compose.prod.yml` - images are pre-built! ✅

---

## Quick Reference

### URLs

| Service | URL |
|---------|-----|
| GraphQL IDE | http://localhost:3000 |
| Tutorial | http://localhost:3001 |
| Admin Dashboard | http://localhost:3002 |
| GraphQL API | http://localhost:8000/graphql |

### Commands

```bash
make prod-start              # Start
make prod-examples-start     # Start all examples
make prod-status             # Health check
make prod-logs               # View logs
make prod-stop               # Stop
make prod-clean              # Remove data
```

### Ports

| Service | Port |
|---------|------|
| FraiseQL Server | 8000 |
| PostgreSQL (Blog) | 5432 |
| PostgreSQL (E-Comm) | 5433 |
| PostgreSQL (Stream) | 5434 |
| GraphQL IDEs | 3000, 3100, 3200 |
| Tutorial | 3001 |
| Admin Dashboard | 3002 |

### Credentials

| Service | User | Password |
|---------|------|----------|
| PostgreSQL | fraiseql | fraiseql_dev |

---

## Feedback & Issues

Found a bug? Have a suggestion?

**Report issues**: https://github.com/anthropics/fraiseql/issues

**Contribute**: Pull requests welcome! See CONTRIBUTING.md

---

## Summary

You now have:
✅ FraiseQL running locally
✅ Interactive GraphQL IDE
✅ Guided tutorial
✅ Admin dashboard for debugging
✅ 3 example applications
✅ Production-ready setup

**Next**: Open http://localhost:3001 and start learning! 🚀

---

**Questions?** Check the tutorial or admin dashboard troubleshooting guides.

**Ready to deploy?** See `.docker-phase5-status.md` for production guidance.
