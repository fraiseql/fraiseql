# FraiseQL Documentation

Welcome to the FraiseQL documentation hub! This directory contains comprehensive documentation organized by user journey and expertise level.

## 🎯 Documentation Philosophy

Our documentation follows **Progressive Disclosure** principles:
- **Multiple Entry Points**: Start from where you are in your journey
- **Layered Learning**: From quick start to advanced patterns
- **Workflow-Oriented**: Organized by what you want to accomplish
- **Always Current**: Documentation evolves with the codebase

## 🗺️ Navigation by User Journey

### 🚀 New to FraiseQL?
**Start here for quickest path to productivity**

```
📍 START HERE
├── getting-started/          # 0-60 in 5 minutes
│   ├── installation.md      # Quick install & first query
│   ├── first-api.md         # Build your first API
│   └── key-concepts.md      # Essential concepts overview
├── tutorials/               # Step-by-step guided learning
│   ├── blog-api-tutorial.md # Complete API from scratch
│   └── advanced-patterns.md # Beyond the basics
└── examples/                # Working code you can run
    └── → See ../examples/   # Live examples directory
```

**Time Investment**: 30 minutes to working API

### 🛠️ Building Production APIs?
**Architecture, patterns, and best practices**

```
📍 PRODUCTION READY
├── architecture/            # System design & patterns
│   ├── cqrs-patterns.md    # Command Query Responsibility Segregation
│   ├── database-design.md   # PostgreSQL optimization
│   └── decisions/          # Architectural Decision Records (ADRs)
├── core-concepts/          # Deep-dive into FraiseQL concepts
│   ├── type-system.md      # Type system & validation
│   ├── mutations.md        # Mutation patterns & error handling
│   └── performance.md      # Performance optimization
└── deployment/             # Production deployment
    ├── docker.md           # Container deployment
    ├── monitoring.md       # Observability & metrics
    └── scaling.md          # Horizontal scaling patterns
```

**Use Cases**: Enterprise APIs, microservices, high-performance systems

### 🔍 Looking for Specific Information?
**Reference materials and troubleshooting**

```
📍 REFERENCE & TROUBLESHOOTING
├── api-reference/          # Complete API documentation
│   ├── decorators.md       # @fraiseql.query, @fraiseql.mutation
│   ├── types.md            # Built-in and custom types
│   └── utilities.md        # Helper functions & utilities
├── errors/                 # Error handling & troubleshooting
│   ├── common-errors.md    # Frequent issues & solutions
│   └── debugging.md        # Debugging techniques
└── migration/              # Version migration guides
    ├── v0.5-migration.md   # Upgrading to v0.5
    └── breaking-changes.md # All breaking changes log
```

**Use Cases**: API reference, debugging issues, version upgrades

### 🚀 Advanced Use Cases?
**Extending FraiseQL for complex scenarios**

```
📍 ADVANCED & EXTENDING
├── advanced/               # Advanced patterns & techniques
│   ├── performance-optimization-layers.md # Three-layer performance architecture
│   ├── apq-storage-backends.md # APQ storage backend abstraction
│   ├── custom-scalars.md   # Building custom scalar types
│   ├── middleware.md       # Custom middleware patterns
│   └── extensions.md       # Framework extensions
├── comparisons/            # vs other GraphQL frameworks
│   ├── vs-graphene.md      # Migration from Graphene
│   └── vs-strawberry.md    # Comparison with Strawberry
└── environmental-impact/   # Sustainability considerations
    └── performance-impact.md
```

**Use Cases**: Framework extension, migration planning, sustainability

### 🧪 Contributing & Development?
**Internal development and contribution guides**

```
📍 DEVELOPMENT & CONTRIBUTING
├── development/            # Internal development documentation
│   ├── setup.md           # Development environment setup
│   ├── testing.md         # Testing strategies & patterns
│   ├── fixes/             # Bug fix documentation
│   ├── planning/          # Development planning docs
│   └── agent-prompts/     # AI assistant prompts
├── testing/               # Testing documentation
│   ├── strategy.md        # Overall testing approach
│   └── patterns.md        # Common testing patterns
└── releases/              # Release documentation
    ├── release-process.md  # How releases are made
    └── changelog.md       # Human-readable changes
```

**Use Cases**: Contributing code, understanding internals, release management

## 🎯 Quick Access by Task

### "I want to..."

#### **Get Started Fast**
→ `getting-started/installation.md` → `tutorials/blog-api-tutorial.md` → `examples/`

#### **Build a Production API**
→ `core-concepts/` → `architecture/` → `deployment/`

#### **Debug an Issue**
→ `errors/common-errors.md` → `api-reference/` → `development/testing.md`

#### **Migrate Versions**
→ `migration/` → `releases/changelog.md` → `errors/`

#### **Extend the Framework**
→ `advanced/` → `development/` → `architecture/decisions/`

#### **Contribute to Project**
→ `development/setup.md` → `testing/` → `../CONTRIBUTING.md`

## 📊 Documentation Maturity Levels

### 🟢 Complete & Current
**Actively maintained, comprehensive coverage**
- `getting-started/` - New user onboarding
- `core-concepts/` - Framework fundamentals
- `api-reference/` - Complete API documentation
- `examples/` - Working code examples
- `releases/` - Release notes and migration guides

### 🟡 Good & Stable
**Solid coverage, periodic updates**
- `tutorials/` - Step-by-step guides
- `architecture/` - Design documentation
- `deployment/` - Production guidance
- `testing/` - Testing approaches

### 🟠 Growing & Evolving
**Active development, expanding coverage**
- `advanced/` - Advanced patterns
- `development/` - Internal documentation
- `comparisons/` - Framework comparisons
- `errors/` - Troubleshooting guides

## 🔧 Documentation Maintenance

### For Contributors
**Adding new documentation:**
1. **Identify audience**: New user? Advanced developer? Contributor?
2. **Choose location**: Use the journey-based organization above
3. **Follow templates**: Use existing documents as templates
4. **Cross-reference**: Link to related documentation
5. **Test examples**: Ensure all code examples work

### For Maintainers
**Regular maintenance tasks:**
- **Update examples**: Keep code examples current with latest version
- **Review accuracy**: Validate documentation matches current behavior
- **Fix broken links**: Regular link checking and repair
- **User feedback**: Incorporate user suggestions and questions
- **Metrics review**: Analyze most/least used documentation

### Documentation Standards
- **Code examples**: All code must be tested and working
- **Screenshots**: Keep UI screenshots current
- **Links**: Use relative links within documentation
- **Structure**: Follow established heading hierarchy
- **Language**: Clear, concise, jargon-free where possible

## 🌟 Getting Help with Documentation

### Finding Information
1. **Start with README files**: Each directory has organization overview
2. **Use search**: Full-text search across all documentation
3. **Follow cross-references**: Documentation is heavily interlinked
4. **Check examples**: Working code often answers questions

### Improving Documentation
- **Report issues**: Use GitHub issues for documentation problems
- **Suggest improvements**: PRs welcome for clarifications and additions
- **Ask questions**: Questions often reveal documentation gaps

---

## 🎯 Quick Start Paths

**Never used FraiseQL?** → `getting-started/installation.md`
**Migrating from another framework?** → `comparisons/` + `migration/`
**Building enterprise API?** → `architecture/` + `deployment/`
**Contributing to FraiseQL?** → `development/setup.md` + `../CONTRIBUTING.md`
**Debugging an issue?** → `errors/common-errors.md`

---

*This documentation architecture evolves with FraiseQL and user needs. When in doubt, start with `getting-started/` and follow the breadcrumbs!*
