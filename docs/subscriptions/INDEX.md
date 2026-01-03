# GraphQL Subscriptions Documentation

Complete guide to using GraphQL subscriptions in FraiseQL.

---

## Quick Navigation

### Getting Started (5 minutes)
📖 **[Getting Started Guide](01-getting-started.md)**
- Installation instructions
- 5-minute quick start
- Key concepts
- Framework choices
- Common patterns
- FAQ

Start here if you're new to GraphQL subscriptions!

---

### API Reference
📚 **[API Reference](02-api-reference.md)**
- SubscriptionManager API
- Configuration options
- Resolver functions
- Error types
- Response formats
- Best practices

Use this when building your application.

---

### Architecture & Design
🏗️ **[Architecture Guide](03-architecture.md)**
- System overview
- Component responsibilities
- Data flow diagrams
- Performance characteristics
- Concurrency model
- Scalability considerations
- Security architecture

Understand how the system works internally.

---

### Deployment & Operations
🚀 **[Deployment Guide](04-deployment.md)**
- Development setup
- Single-server production
- Multi-server production
- Event bus configuration
- Docker deployment
- Kubernetes deployment
- Load balancing
- Performance tuning
- Monitoring & observability
- Production checklist

Deploy to production with confidence.

---

### Troubleshooting
🔧 **[Troubleshooting Guide](05-troubleshooting.md)**
- Common issues and solutions
- Debugging techniques
- Performance optimization
- Memory leak detection
- FAQ
- When to use Redis vs PostgreSQL

Solve problems quickly.

---

## Code Examples

### FastAPI Integration
💡 **[FastAPI Example](../examples/subscriptions/fastapi_example.py)**
- Complete working application
- WebSocket endpoint
- REST endpoints for publishing
- HTML test client
- Real-time updates

**Run**:
```bash
uvicorn fastapi_example:app --reload
open http://localhost:8000
```

---

### Starlette Integration
💡 **[Starlette Example](../examples/subscriptions/starlette_example.py)**
- Same features as FastAPI
- Demonstrates framework independence
- Lightweight ASGI app
- WebSocket handling

**Run**:
```bash
uvicorn starlette_example:app --reload
```

---

### Custom Adapter Template
💡 **[Custom Adapter](../examples/subscriptions/custom_adapter.py)**
- Abstract base classes
- Framework integration pattern
- Resolver mapping
- Step-by-step integration guide

Use this to add subscriptions to your framework.

---

### Real-World Chat Application
💡 **[Chat Application](../examples/subscriptions/realworld_chat.py)**
- Multi-user chat room
- User presence tracking
- Message history
- Typing indicators
- Production-ready patterns

Copy patterns for your application.

---

## Learning Path

### Beginner
1. [Getting Started](01-getting-started.md) - Learn the basics
2. [FastAPI Example](../examples/subscriptions/fastapi_example.py) - See it in action
3. [API Reference](02-api-reference.md) - Understand the APIs

**Time**: ~1 hour

### Intermediate
1. [Architecture Guide](03-architecture.md) - Understand the design
2. [Chat Example](../examples/subscriptions/realworld_chat.py) - Copy patterns
3. [Deployment Guide](04-deployment.md) - Prepare for production

**Time**: ~2 hours

### Advanced
1. [Performance Optimization](05-troubleshooting.md#performance-optimization)
2. [Custom Adapter](../examples/subscriptions/custom_adapter.py) - Integrate with other frameworks
3. [Deployment Checklist](04-deployment.md#production-checklist)

**Time**: ~3 hours

---

## Cheat Sheet

### Minimal Code Example

```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# Setup
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)

# Create resolver
async def my_resolver(event, variables):
    return {"data": event}

# Subscribe
await manager.create_subscription(
    subscription_id="sub1",
    connection_id="ws1",
    query="subscription { data }",
    variables={},
    resolver_fn=my_resolver,
    user_id="user1",
    tenant_id="tenant1"
)

# Publish
await manager.publish_event(
    event_type="test",
    channel="test",
    data={"test": "value"}
)

# Receive
response = await manager.get_next_event("sub1")
```

---

## Key Concepts at a Glance

| Concept | Meaning |
|---------|---------|
| **Subscription** | Client listening for real-time updates |
| **Event** | Data published to a channel |
| **Resolver** | Python function transforming events |
| **Channel** | Named stream of events |
| **Event Bus** | System distributing events (memory/Redis/PostgreSQL) |
| **Tenant** | Organization/workspace for multi-tenancy |

---

## Performance Targets

| Metric | Target |
|--------|--------|
| Subscription creation | <2ms |
| Event dispatch | <1ms per 100 subs |
| Python resolver | <100μs |
| End-to-end latency | <10ms |
| Throughput | >10k events/sec |
| Concurrent subscriptions | 1000+ per server |

---

## Event Bus Comparison

| Feature | Memory | Redis | PostgreSQL |
|---------|--------|-------|------------|
| **Speed** | Fastest | Fast | Slower |
| **Multi-server** | ❌ | ✅ | ✅ |
| **Persistence** | ❌ | Optional | ✅ |
| **Setup** | Simple | Moderate | Simple |
| **Throughput** | >100k/s | >50k/s | >10k/s |
| **Use Case** | Dev | Production | Persistence |

---

## FAQ

**Q: Do I need to know GraphQL?**
A: No, just write Python resolver functions.

**Q: Can I use subscriptions with REST?**
A: No, subscriptions are GraphQL-specific. Use webhooks for REST.

**Q: How do I scale to multiple servers?**
A: Use Redis event bus instead of memory.

**Q: Is it secure?**
A: Yes! Built-in security filtering by user/tenant.

**Q: What's the latency?**
A: <10ms end-to-end, usually <5ms.

See [Getting Started FAQ](01-getting-started.md#faq) for more.

---

## Support & Feedback

- 📖 Check the relevant documentation section
- 🔧 See [Troubleshooting Guide](05-troubleshooting.md)
- 💬 Open an issue on GitHub
- 📧 Contact support

---

## Document Overview

```
subscriptions/
├── INDEX.md (you are here)
├── 01-getting-started.md      (5 min read)
├── 02-api-reference.md         (reference)
├── 03-architecture.md          (15 min read)
├── 04-deployment.md            (reference)
└── 05-troubleshooting.md       (reference)

examples/subscriptions/
├── fastapi_example.py          (run immediately)
├── starlette_example.py        (alternative framework)
├── custom_adapter.py           (integration template)
└── realworld_chat.py           (production patterns)
```

---

## Next Steps

1. **New to subscriptions?** → [Getting Started](01-getting-started.md)
2. **Ready to build?** → [FastAPI Example](../examples/subscriptions/fastapi_example.py)
3. **Going to production?** → [Deployment Guide](04-deployment.md)
4. **Have issues?** → [Troubleshooting](05-troubleshooting.md)

Happy building! 🚀

---

**Last Updated**: January 3, 2026
**Version**: FraiseQL 1.9.1
