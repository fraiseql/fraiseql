# Fraisier Documentation Index

Complete documentation for the Fraisier deployment orchestrator and reference implementation.

---

## Quick Navigation

### For Getting Started

- **New to Fraisier?** Start with [../README.md](../README.md)
- **Setting up development?** See [../development.md](../development.md)
- **Your first deployment?** See [getting-started-docker.md](getting-started-docker.md) (10 min) or [getting-started-sqlite.md](getting-started-sqlite.md) (5 min)

### For Production Deployment

- **Quick setup (Docker)?** See [getting-started-docker.md](getting-started-docker.md)
- **Setup guide (your DB)?** See [getting-started-postgres.md](getting-started-postgres.md), [getting-started-mysql.md](getting-started-mysql.md), [getting-started-sqlite.md](getting-started-sqlite.md)
- **Your deployment method?** See [provider-bare-metal.md](provider-bare-metal.md), [provider-docker-compose.md](provider-docker-compose.md), or [provider-coolify.md](provider-coolify.md)
- **Real-world examples?** See [real-world-examples.md](real-world-examples.md) (4 complete configurations)

### For API & Integration

- **REST API?** See [api-reference.md](api-reference.md)
- **CLI commands?** See [cli-reference.md](cli-reference.md)
- **Webhooks?** See [webhook-reference.md](webhook-reference.md)
- **Event types?** See [event-reference.md](event-reference.md)

### For Development

- **Project standards?** See [../.claude/CLAUDE.md](../.claude/CLAUDE.md)
- **What needs building?** See [../roadmap.md](../roadmap.md)
- **Writing tests?** See [testing.md](testing.md)
- **Understanding architecture?** See [architecture.md](architecture.md)

### For Operations

- **Monitoring setup?** See [monitoring-setup.md](monitoring-setup.md)
- **Troubleshooting?** See [troubleshooting.md](troubleshooting.md) (50+ scenarios)
- **FAQ?** See [faq-and-advanced-topics.md](faq-and-advanced-topics.md) (40+ Q&A)

---

## Documentation Files

### In this `docs/` Directory

#### Core Documentation

| File | Purpose | Audience |
|------|---------|----------|
| **architecture.md** | Detailed technical architecture | Engineers, Architects |
| **deployment-guide.md** | Production deployment instructions | DevOps, Operators |
| **testing.md** | Testing strategy and examples | Engineers, QA |
| **prd.md** | Product requirements document | Product Managers, Architects |

#### Phase 10: Complete Documentation System (v0.1.0)

##### 10.1: API Reference Documentation

| File | Purpose | Size | Audience |
|------|---------|------|----------|
| **api-reference.md** | Complete REST API endpoints, examples, SDKs | 25K | Developers, Integrators |
| **cli-reference.md** | 40+ CLI commands with options and examples | 18K | DevOps, Operators |
| **webhook-reference.md** | Webhook configuration, security, integrations | 19K | DevOps, Operators |
| **event-reference.md** | NATS event types, filtering, replay patterns | 17K | Developers, DevOps |

##### 10.2: Getting Started Guides

| File | Purpose | Time | Audience |
|------|---------|------|----------|
| **getting-started-sqlite.md** | Local development setup | 5-10 min | Everyone |
| **getting-started-postgres.md** | Production PostgreSQL with HA | 20-30 min | DevOps |
| **getting-started-mysql.md** | MySQL 8.0+ configuration | 20-30 min | DevOps |
| **getting-started-docker.md** | Full Docker Compose stack | 15-20 min | Everyone |

##### 10.3: Provider Setup Guides

| File | Purpose | Time | Audience |
|------|---------|------|----------|
| **provider-bare-metal.md** | SSH + systemd deployment | 20-25 min | DevOps |
| **provider-docker-compose.md** | Docker Compose deployments | 10-15 min | DevOps, Developers |
| **provider-coolify.md** | Coolify PaaS integration | 20-25 min | DevOps |

##### 10.4: Monitoring & Operations

| File | Purpose | Audience |
|------|---------|----------|
| **monitoring-setup.md** | Prometheus, Grafana, alerting rules | DevOps, SREs |

##### 10.5: Troubleshooting & Help

| File | Purpose | Scenarios | Audience |
|------|---------|-----------|----------|
| **troubleshooting.md** | 50+ common issues with debug commands | 50+ | Everyone |

##### 10.6: Real-World Examples

| File | Purpose | Examples | Audience |
|------|---------|----------|----------|
| **real-world-examples.md** | 4 production configurations with code | 4 | DevOps, Architects |

##### 10.7: FAQ & Advanced Topics

| File | Purpose | Q&A | Audience |
|------|---------|-----|----------|
| **faq-and-advanced-topics.md** | 40+ FAQ, custom providers, performance tuning | 40+ | Everyone |

### In Parent `fraisier/` Directory

| File | Purpose | Audience |
|------|---------|----------|
| **README.md** | Quick start and overview | Everyone |
| **development.md** | Development setup and workflow | Engineers |
| **roadmap.md** | Development phases and priorities | Engineers, Managers |
| **.claude/CLAUDE.md** | Project standards and principles | Engineers |

---

## By Role

### I'm a Developer

1. **Getting started**: [../development.md](../development.md)
2. **Understanding code**: [architecture.md](architecture.md)
3. **Writing tests**: [testing.md](testing.md)
4. **Project standards**: [../.claude/CLAUDE.md](../.claude/CLAUDE.md)
5. **What to build**: [../roadmap.md](../roadmap.md)

### I'm a DevOps / Operator

1. **Deploying**: [deployment-guide.md](deployment-guide.md)
2. **Monitoring**: [deployment-guide.md#monitoring--logs](deployment-guide.md#monitoring--logs)
3. **Troubleshooting**: [deployment-guide.md#troubleshooting](deployment-guide.md#troubleshooting)
4. **Backup/Recovery**: [deployment-guide.md#backup--disaster-recovery](deployment-guide.md#backup--disaster-recovery)

### I'm a Product Manager

1. **Vision and goals**: [prd.md](prd.md)
2. **Development phases**: [../roadmap.md](../roadmap.md)
3. **Current status**: [../README.md](../README.md)

### I'm Learning FraiseQL

1. **Start here**: [../README.md](../README.md)
2. **How it works**: [architecture.md](architecture.md)
3. **Example code**: Look in `../fraisier/` package
4. **Full requirements**: [prd.md](prd.md)

---

## Documentation Structure

```
fraisier/
├── README.md                    ← START HERE (overview)
├── development.md               (setup & dev workflow)
├── roadmap.md                   (development phases)
│
├── .claude/
│   └── CLAUDE.md                (project standards)
│
├── docs/
│   ├── index.md                 (this file)
│   ├── architecture.md           (technical deep-dive)
│   ├── deployment-guide.md       (production ops)
│   ├── testing.md                (testing strategy)
│   └── prd.md                    (product requirements)
│
└── fraisier/
    └── (Python package source)
```

---

## Document Purposes

### README.md

**What**: Quick start and project overview
**When to read**: First encounter with Fraisier
**Length**: ~250 lines
**Content**: Purpose, quick start, config examples, architecture diagram

### development.md

**What**: How to set up development environment and workflow
**When to read**: Getting started with development
**Length**: ~400 lines
**Content**: Prerequisites, setup, common tasks, testing, debugging, IDE setup

### roadmap.md

**What**: What features are planned and in what order
**When to read**: Understanding project status and direction
**Length**: ~300 lines
**Content**: Vision, 4 phases, current blockers, success criteria, version history

### .claude/CLAUDE.md

**What**: Development standards, principles, and conventions specific to Fraisier
**When to read**: Before writing code
**Length**: ~400 lines
**Content**: Architecture principles, testing strategy, common tasks, code patterns, debugging

### architecture.md

**What**: Technical architecture, component overview, data flow
**When to read**: Understanding how code is organized
**Length**: ~500 lines
**Content**: High-level diagram, component details, data flows, design patterns, extension points

### deployment-guide.md

**What**: How to deploy Fraisier and manage services in production
**When to read**: Before deploying to production
**Length**: ~600 lines
**Content**: Prerequisites, installation, configuration, systemd setup, webhooks, monitoring, troubleshooting

### testing.md

**What**: Testing strategy, examples, and best practices
**When to read**: Before writing tests
**Length**: ~400 lines
**Content**: Test structure, how to run tests, unit/integration/E2E examples, mocking, coverage

### prd.md (This directory)

**What**: Complete product requirements and specifications
**When to read**: Understanding full scope and features
**Length**: ~1,300 lines
**Content**: Vision, vocabulary, architecture patterns, configuration schema, GraphQL API, roadmap

---

## Quick Reference Searches

### How do I

| Question | Find Answer In |
|----------|---|
| Set up Fraisier for development? | [../development.md](../development.md) |
| Write a test? | [testing.md](testing.md) |
| Deploy to production? | [deployment-guide.md](deployment-guide.md) |
| Understand the code? | [architecture.md](architecture.md) |
| Add a new CLI command? | [../.claude/CLAUDE.md](../.claude/CLAUDE.md#adding-a-new-cli-command) |
| Add a new deployer? | [../.claude/CLAUDE.md](../.claude/CLAUDE.md#adding-a-new-deployer-type) |
| Add a new Git provider? | [../.claude/CLAUDE.md](../.claude/CLAUDE.md#adding-a-new-git-provider) |
| Set up webhooks? | [deployment-guide.md#git-webhook-configuration](deployment-guide.md#git-webhook-configuration) |
| Debug deployment failures? | [deployment-guide.md#troubleshooting](deployment-guide.md#troubleshooting) |
| See the project roadmap? | [../roadmap.md](../roadmap.md) |

---

## Related Documentation

### FraiseQL Framework

- **Framework README**: See `../../README.md` (parent repo)
- **Framework Architecture**: See `../../docs/`
- **Language Bindings**: See `../../fraiseql-python/`, `../../fraiseql-typescript/`, etc.

### GitHub Repository

- **Main Repo**: https://github.com/fraiseql/fraiseql
- **Issues**: https://github.com/fraiseql/fraiseql/issues
- **Discussions**: https://github.com/fraiseql/fraiseql/discussions

---

## Document Versions

| Document | Last Updated | Version |
|----------|---|---|
| README.md | 2026-01-22 | 0.1.0 |
| development.md | 2026-01-22 | 0.1.0 |
| roadmap.md | 2026-01-22 | 0.1.0 |
| .claude/CLAUDE.md | 2026-01-22 | 0.1.0 |
| architecture.md | 2026-01-22 | 0.1.0 |
| deployment-guide.md | 2026-01-22 | 0.1.0 |
| testing.md | 2026-01-22 | 0.1.0 |
| prd.md | 2026-01-15 | 1.0.0 |
| **api-reference.md** | **2026-01-22** | **0.1.0** |
| **cli-reference.md** | **2026-01-22** | **0.1.0** |
| **webhook-reference.md** | **2026-01-22** | **0.1.0** |
| **event-reference.md** | **2026-01-22** | **0.1.0** |
| **getting-started-sqlite.md** | **2026-01-22** | **0.1.0** |
| **getting-started-postgres.md** | **2026-01-22** | **0.1.0** |
| **getting-started-mysql.md** | **2026-01-22** | **0.1.0** |
| **getting-started-docker.md** | **2026-01-22** | **0.1.0** |
| **provider-bare-metal.md** | **2026-01-22** | **0.1.0** |
| **provider-docker-compose.md** | **2026-01-22** | **0.1.0** |
| **provider-coolify.md** | **2026-01-22** | **0.1.0** |
| **monitoring-setup.md** | **2026-01-22** | **0.1.0** |
| **troubleshooting.md** | **2026-01-22** | **0.1.0** |
| **real-world-examples.md** | **2026-01-22** | **0.1.0** |
| **faq-and-advanced-topics.md** | **2026-01-22** | **0.1.0** |

---

## Contributing to Documentation

### Before Editing

1. Read this INDEX to understand structure
2. Check document purpose and audience
3. Maintain consistent tone and style

### Style Guide

- Clear, concise language
- Active voice preferred
- Use headers for organization
- Include code examples where helpful
- Link to related documentation

### Sections to Update

- Update document version above when making changes
- Update "Last Updated" date
- Keep index.md synchronized with new files

---

## How to Navigate

1. **Know your audience**: Find your role above
2. **Read recommended docs** in order
3. **Use Quick Reference** to jump to specific topics
4. **Follow links** to related documentation
5. **Search**: Use Ctrl+F to find specific terms

---

## Getting Help

- **Development questions**: See [../.claude/CLAUDE.md](../.claude/CLAUDE.md#getting-help)
- **Deployment questions**: See [deployment-guide.md#troubleshooting](deployment-guide.md#troubleshooting)
- **Design questions**: See [architecture.md](architecture.md)
- **Testing help**: See [testing.md](testing.md)

---

**Last Updated**: 2026-01-22
**Maintained by**: FraiseQL Team
