# FraiseQL Examples Hub

Welcome to the FraiseQL examples collection! This directory contains 20+ comprehensive example applications demonstrating FraiseQL's capabilities across different domains and use cases.

## 🚀 Quick Start

**New to FraiseQL? Start here:**
- **[📚 Examples Index](INDEX.md)** - Complete organized catalog of all examples
- **[🎯 Learning Paths](LEARNING_PATHS.md)** - Structured progression from beginner to expert
- **[`todo_quickstart.py`](todo_quickstart.py)** - 5-minute introduction to basic GraphQL API

## 📖 Navigation

| Document | Purpose | Best For |
|----------|---------|----------|
| **[INDEX.md](INDEX.md)** | Complete catalog by difficulty and use case | Finding specific examples |
| **[LEARNING_PATHS.md](LEARNING_PATHS.md)** | Structured learning progression | Following guided paths |
| **[This README](README.md)** | Overview and legacy content | Understanding scope |

## 🎯 Popular Starting Points

### 🟢 Beginner Friendly
- **[`todo_quickstart.py`](todo_quickstart.py)** - Simple todo app (5 min)
- **[`blog_api/`](blog_api/)** - Content management with enterprise patterns (15 min)
- **[`health_check_example.py`](health_check_example.py)** - Basic endpoints (5 min)

### 🏢 Production Ready
- **[`enterprise_patterns/`](enterprise_patterns/)** - All enterprise patterns (45 min)
- **[`ecommerce/`](ecommerce/)** - Complete e-commerce platform (30 min)
- **[`saas-starter/`](saas-starter/)** - Multi-tenant SaaS foundation (50 min)

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

See **[INDEX.md](INDEX.md)** for the complete organized catalog.

## 🏢 Enterprise Patterns (`enterprise_patterns/`)

**The definitive reference for production-ready enterprise applications.**

Complete showcase of all FraiseQL enterprise patterns including mutation results, audit trails, multi-layer validation, and compliance features.

**⏱️ Time: 45 min** | **🏷️ Difficulty: Advanced** | **🎯 Use Case: Enterprise**

See **[INDEX.md](INDEX.md)** for setup instructions and related examples.

## 🏪 E-commerce (`ecommerce/`)

Complete e-commerce platform with product catalog, shopping cart, orders, reviews, and search.

**⏱️ Time: 30 min** | **🏷️ Difficulty: Intermediate** | **🎯 Use Case: E-commerce**

See **[INDEX.md](INDEX.md)** for setup instructions and related examples.

## 💬 Real-time Chat (`real_time_chat/`)

WebSocket-based messaging with presence tracking, typing indicators, and real-time features.

**⏱️ Time: 45 min** | **🏷️ Difficulty: Advanced** | **🎯 Use Case: Real-time**

## 📊 Analytics Dashboard (`analytics_dashboard/`)

Business intelligence platform with time-series analytics and performance monitoring.

**⏱️ Time: 40 min** | **🏷️ Difficulty: Advanced** | **🎯 Use Case: Analytics**

## 📝 Blog API (`blog_api/`)

Content management with enterprise patterns, authentication, and audit trails.

**⏱️ Time: 15 min** | **🏷️ Difficulty: Beginner** | **🎯 Use Case: Content Management**

See **[INDEX.md](INDEX.md)** for complete details and setup instructions.

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
- Follow the structure in [`_TEMPLATE_README.md`](_TEMPLATE_README.md)
- Include comprehensive documentation and tests
- Update [INDEX.md](INDEX.md) with new examples

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

*This examples hub provides organized access to 20+ FraiseQL examples. Use [INDEX.md](INDEX.md) to find specific examples or [LEARNING_PATHS.md](LEARNING_PATHS.md) for structured learning progression.*
