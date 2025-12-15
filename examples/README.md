# FraiseQL Examples Hub

Welcome to the FraiseQL examples collection! This directory contains 35+ comprehensive example applications demonstrating FraiseQL's capabilities across different domains and use cases.

## ✅ Trinity Pattern Compliance

All examples follow the [Trinity Pattern](../docs/guides/trinity-pattern-guide.md) - FraiseQL's three-identifier system for optimal performance, security, and UX.

**Verification Status**: All examples are automatically verified for pattern compliance.

| Compliance Level | Badge | Description |
|------------------|-------|-------------|
| **100% Compliant** | 🟢 | Perfect Trinity implementation |
| **95%+ Compliant** | 🟡 | Minor warnings acceptable |
| **<95% Compliant** | 🔴 | Needs remediation |

**Run verification on any example:**
```bash
python .phases/verify-examples-compliance/verify.py examples/blog_api/
```

## 🚀 Quick Start

**New to FraiseQL? Start here:**
- **[📚 Examples Index](index.md)** - Complete organized catalog of all examples
- **[🎯 Learning Paths](learning-paths.md)** - Structured progression from beginner to expert
- **[`todo_quickstart.py`](todo_quickstart.py)** - 5-minute introduction to basic GraphQL API

## 📖 Navigation

| Document | Purpose | Best For |
|----------|---------|----------|
| **[index.md](index.md)** | Complete catalog by difficulty and use case | Finding specific examples |
| **[learning-paths.md](learning-paths.md)** | Structured learning progression | Following guided paths |
| **[This README](README.md)** | Overview and legacy content | Understanding scope |

## 🎯 Popular Starting Points

### 🟢 Beginner Friendly (100% Compliant)
- **[`todo_xs/`](todo_xs/)** 🟢 - Minimal todo app with perfect Trinity (10 min)
- **[`blog_api/`](blog_api/)** 🟢 - Content management with enterprise patterns (15 min)
- **[`health_check_example.py`](health_check_example.py)** - Basic endpoints (5 min)

### 🏢 Production Ready (95%+ Compliant)
- **[`enterprise_patterns/`](enterprise_patterns/)** 🟡 - All enterprise patterns (45 min)
- **[`ecommerce_api/`](ecommerce_api/)** 🟡 - Complete e-commerce platform (30 min)
- **[`real_time_chat/`](real_time_chat/)** 🟢 - Real-time chat with subscriptions (25 min)

## 🏗️ Example Categories

### By Difficulty
- **🟢 Beginner** (4 examples) - Learn FraiseQL fundamentals
- **🟡 Intermediate** (8 examples) - Build real-world applications
- **🟠 Advanced** (6 examples) - Enterprise-grade patterns
- **🔴 Specialized** (4 examples) - Domain-specific solutions

### By Use Case
- **🛍️ E-commerce & Business** - Online stores, analytics, admin panels
- **📝 Content Management** - Blogs, CMS, document systems
- **🔐 Authentication & Security** - Auth patterns, token management
- **⚡ Performance & Caching** - Optimization, APQ, query routing
- **🏢 Enterprise Patterns** - Compliance, multi-tenancy, audit trails

See **[index.md](index.md)** for the complete organized catalog.

## 🆕 Creating New Examples

**Use the template for guaranteed compliance:**

```bash
# Copy the template
cp -r examples/_TEMPLATE examples/my-awesome-example

# Follow the Trinity checklist in _TEMPLATE/README.md
# Run verification before submitting
python .phases/verify-examples-compliance/verify.py examples/my-awesome-example/

# Should show: ✅ Compliance: 100%
```

**Template includes:**
- ✅ Complete Trinity pattern setup
- ✅ Verification checklist
- ✅ Example structure and documentation
- ✅ CI-ready configuration

See [Contributing Guide](../CONTRIBUTING.md#adding-examples) for details.

## 🏢 Enterprise Patterns (`enterprise_patterns/`)

**The definitive reference for production-ready enterprise applications.**

Complete showcase of all FraiseQL enterprise patterns including mutation results, audit trails, multi-layer validation, and compliance features.

**⏱️ Time: 45 min** | **🏷️ Difficulty: Advanced** | **🎯 Use Case: Enterprise** | **🏷️ Tags: audit, validation, compliance, production**

See **[index.md](index.md)** for setup instructions and related examples.

## 🏪 E-commerce (`ecommerce/`)

Complete e-commerce platform with product catalog, shopping cart, orders, reviews, and search.

**⏱️ Time: 30 min** | **🏷️ Difficulty: Intermediate** | **🎯 Use Case: E-commerce** | **🏷️ Tags: business, transactions, catalog, search**

See **[index.md](index.md)** for setup instructions and related examples.

## 💬 Real-time Chat (`real_time_chat/`)

WebSocket-based messaging with presence tracking, typing indicators, and real-time features.

**⏱️ Time: 45 min** | **🏷️ Difficulty: Advanced** | **🎯 Use Case: Real-time** | **🏷️ Tags: websocket, realtime, messaging, subscriptions**

## 📊 Analytics Dashboard (`analytics_dashboard/`)

Business intelligence platform with time-series analytics and performance monitoring.

**⏱️ Time: 40 min** | **🏷️ Difficulty: Advanced** | **🎯 Use Case: Analytics** | **🏷️ Tags: timeseries, metrics, dashboard, business-intelligence**

## 📝 Blog API (`blog_api/`)

Content management with enterprise patterns, authentication, and audit trails.

**⏱️ Time: 15 min** | **🏷️ Difficulty: Beginner** | **🎯 Use Case: Content Management** | **🏷️ Tags: cms, authentication, crud, enterprise**

See **[index.md](index.md)** for complete details and setup instructions.

## 📈 Performance & Architecture

**Performance benchmarks and architecture overview available in:**
- **[Performance Guide](../docs/performance/)** - Detailed benchmarks and optimization
- **[Architecture Docs](../docs/architecture/)** - CQRS patterns and type system
- **[Core Concepts](../docs/core/)** - Database-first design principles

## 🚀 Getting Started

### Prerequisites
- **Python 3.10+** (for modern type syntax: `list[Type]`, `Type | None`)
- **PostgreSQL 13+**
- Docker & Docker Compose (optional)

### Installation
```bash
# Clone the repository
git clone https://github.com/your-org/fraiseql.git
cd fraiseql/examples

# Choose an example
cd ecommerce_api

# Install dependencies
pip install -r requirements.txt

# Set up database
createdb ecommerce
psql -d ecommerce -f db/migrations/001_initial_schema.sql

# Run the application
uvicorn app:app --reload
```

## 🛠️ Development & Testing

**Tools and best practices:**
- **[Development Tools](../docs/development/)** - GraphQL playground, database tools, testing
- **[Best Practices](../docs/core/)** - Database design, API design, security, performance
- **[Debugging Guide](../docs/production/)** - Monitoring, query analysis, troubleshooting

## 🤝 Contributing Examples

**Adding new examples:**
- Follow the structure in [`_template-readme.md`](_template-readme.md)
- Include comprehensive documentation and tests
- Update [index.md](index.md) with new examples

## 📖 Documentation Links

- **[Main Documentation](../docs/)** - Complete FraiseQL reference
- **[Quick Start](../docs/getting-started/quickstart.md)** - Getting started guide
- **[Core Concepts](../docs/core/)** - Fundamental patterns
- **[Performance Guide](../docs/performance/)** - Optimization techniques
- **[Production Deployment](../docs/production/)** - Production setup

## 🆘 Support

- **Issues**: [GitHub Issues](../../issues)
- **Discussions**: [GitHub Discussions](../../discussions)
- **Discord**: [FraiseQL Community](https://discord.gg/fraiseql)

---

*This examples hub provides organized access to 20+ FraiseQL examples. Use [index.md](index.md) to find specific examples or [learning-paths.md](learning-paths.md) for structured learning progression.*
