# Fraisier Documentation Index

Complete documentation for the Fraisier deployment orchestrator and reference implementation.

---

## Quick Navigation

### For Getting Started

- **New to Fraisier?** Start with [../README.md](../README.md)
- **Setting up development?** See [../DEVELOPMENT.md](../DEVELOPMENT.md)
- **Your first deployment?** See [GETTING_STARTED_DOCKER.md](GETTING_STARTED_DOCKER.md) (10 min) or [GETTING_STARTED_SQLITE.md](GETTING_STARTED_SQLITE.md) (5 min)

### For Production Deployment

- **Quick setup (Docker)?** See [GETTING_STARTED_DOCKER.md](GETTING_STARTED_DOCKER.md)
- **Setup guide (your DB)?** See [GETTING_STARTED_POSTGRES.md](GETTING_STARTED_POSTGRES.md), [GETTING_STARTED_MYSQL.md](GETTING_STARTED_MYSQL.md), [GETTING_STARTED_SQLITE.md](GETTING_STARTED_SQLITE.md)
- **Your deployment method?** See [PROVIDER_BARE_METAL.md](PROVIDER_BARE_METAL.md), [PROVIDER_DOCKER_COMPOSE.md](PROVIDER_DOCKER_COMPOSE.md), or [PROVIDER_COOLIFY.md](PROVIDER_COOLIFY.md)
- **Real-world examples?** See [REAL_WORLD_EXAMPLES.md](REAL_WORLD_EXAMPLES.md) (4 complete configurations)

### For API & Integration

- **REST API?** See [API_REFERENCE.md](API_REFERENCE.md)
- **CLI commands?** See [CLI_REFERENCE.md](CLI_REFERENCE.md)
- **Webhooks?** See [WEBHOOK_REFERENCE.md](WEBHOOK_REFERENCE.md)
- **Event types?** See [EVENT_REFERENCE.md](EVENT_REFERENCE.md)

### For Development

- **Project standards?** See [../.claude/CLAUDE.md](../.claude/CLAUDE.md)
- **What needs building?** See [../ROADMAP.md](../ROADMAP.md)
- **Writing tests?** See [TESTING.md](TESTING.md)
- **Understanding architecture?** See [ARCHITECTURE.md](ARCHITECTURE.md)

### For Operations

- **Monitoring setup?** See [MONITORING_SETUP.md](MONITORING_SETUP.md)
- **Troubleshooting?** See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) (50+ scenarios)
- **FAQ?** See [FAQ_AND_ADVANCED_TOPICS.md](FAQ_AND_ADVANCED_TOPICS.md) (40+ Q&A)

---

## Documentation Files

### In this `docs/` Directory

#### Core Documentation

| File | Purpose | Audience |
|------|---------|----------|
| **ARCHITECTURE.md** | Detailed technical architecture | Engineers, Architects |
| **DEPLOYMENT_GUIDE.md** | Production deployment instructions | DevOps, Operators |
| **TESTING.md** | Testing strategy and examples | Engineers, QA |
| **PRD.md** | Product requirements document | Product Managers, Architects |

#### Phase 10: Complete Documentation System (v0.1.0)

##### 10.1: API Reference Documentation

| File | Purpose | Size | Audience |
|------|---------|------|----------|
| **API_REFERENCE.md** | Complete REST API endpoints, examples, SDKs | 25K | Developers, Integrators |
| **CLI_REFERENCE.md** | 40+ CLI commands with options and examples | 18K | DevOps, Operators |
| **WEBHOOK_REFERENCE.md** | Webhook configuration, security, integrations | 19K | DevOps, Operators |
| **EVENT_REFERENCE.md** | NATS event types, filtering, replay patterns | 17K | Developers, DevOps |

##### 10.2: Getting Started Guides

| File | Purpose | Time | Audience |
|------|---------|------|----------|
| **GETTING_STARTED_SQLITE.md** | Local development setup | 5-10 min | Everyone |
| **GETTING_STARTED_POSTGRES.md** | Production PostgreSQL with HA | 20-30 min | DevOps |
| **GETTING_STARTED_MYSQL.md** | MySQL 8.0+ configuration | 20-30 min | DevOps |
| **GETTING_STARTED_DOCKER.md** | Full Docker Compose stack | 15-20 min | Everyone |

##### 10.3: Provider Setup Guides

| File | Purpose | Time | Audience |
|------|---------|------|----------|
| **PROVIDER_BARE_METAL.md** | SSH + systemd deployment | 20-25 min | DevOps |
| **PROVIDER_DOCKER_COMPOSE.md** | Docker Compose deployments | 10-15 min | DevOps, Developers |
| **PROVIDER_COOLIFY.md** | Coolify PaaS integration | 20-25 min | DevOps |

##### 10.4: Monitoring & Operations

| File | Purpose | Audience |
|------|---------|----------|
| **MONITORING_SETUP.md** | Prometheus, Grafana, alerting rules | DevOps, SREs |

##### 10.5: Troubleshooting & Help

| File | Purpose | Scenarios | Audience |
|------|---------|-----------|----------|
| **TROUBLESHOOTING.md** | 50+ common issues with debug commands | 50+ | Everyone |

##### 10.6: Real-World Examples

| File | Purpose | Examples | Audience |
|------|---------|----------|----------|
| **REAL_WORLD_EXAMPLES.md** | 4 production configurations with code | 4 | DevOps, Architects |

##### 10.7: FAQ & Advanced Topics

| File | Purpose | Q&A | Audience |
|------|---------|-----|----------|
| **FAQ_AND_ADVANCED_TOPICS.md** | 40+ FAQ, custom providers, performance tuning | 40+ | Everyone |

### In Parent `fraisier/` Directory

| File | Purpose | Audience |
|------|---------|----------|
| **README.md** | Quick start and overview | Everyone |
| **DEVELOPMENT.md** | Development setup and workflow | Engineers |
| **ROADMAP.md** | Development phases and priorities | Engineers, Managers |
| **.claude/CLAUDE.md** | Project standards and principles | Engineers |

---

## By Role

### I'm a Developer

1. **Getting started**: [../DEVELOPMENT.md](../DEVELOPMENT.md)
2. **Understanding code**: [ARCHITECTURE.md](ARCHITECTURE.md)
3. **Writing tests**: [TESTING.md](TESTING.md)
4. **Project standards**: [../.claude/CLAUDE.md](../.claude/CLAUDE.md)
5. **What to build**: [../ROADMAP.md](../ROADMAP.md)

### I'm a DevOps / Operator

1. **Deploying**: [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md)
2. **Monitoring**: [DEPLOYMENT_GUIDE.md#monitoring--logs](DEPLOYMENT_GUIDE.md#monitoring--logs)
3. **Troubleshooting**: [DEPLOYMENT_GUIDE.md#troubleshooting](DEPLOYMENT_GUIDE.md#troubleshooting)
4. **Backup/Recovery**: [DEPLOYMENT_GUIDE.md#backup--disaster-recovery](DEPLOYMENT_GUIDE.md#backup--disaster-recovery)

### I'm a Product Manager

1. **Vision and goals**: [PRD.md](PRD.md)
2. **Development phases**: [../ROADMAP.md](../ROADMAP.md)
3. **Current status**: [../README.md](../README.md)

### I'm Learning FraiseQL

1. **Start here**: [../README.md](../README.md)
2. **How it works**: [ARCHITECTURE.md](ARCHITECTURE.md)
3. **Example code**: Look in `../fraisier/` package
4. **Full requirements**: [PRD.md](PRD.md)

---

## Documentation Structure

```
fraisier/
├── README.md                    ← START HERE (overview)
├── DEVELOPMENT.md               (setup & dev workflow)
├── ROADMAP.md                   (development phases)
│
├── .claude/
│   └── CLAUDE.md                (project standards)
│
├── docs/
│   ├── INDEX.md                 (this file)
│   ├── ARCHITECTURE.md           (technical deep-dive)
│   ├── DEPLOYMENT_GUIDE.md       (production ops)
│   ├── TESTING.md                (testing strategy)
│   └── PRD.md                    (product requirements)
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

### DEVELOPMENT.md

**What**: How to set up development environment and workflow
**When to read**: Getting started with development
**Length**: ~400 lines
**Content**: Prerequisites, setup, common tasks, testing, debugging, IDE setup

### ROADMAP.md

**What**: What features are planned and in what order
**When to read**: Understanding project status and direction
**Length**: ~300 lines
**Content**: Vision, 4 phases, current blockers, success criteria, version history

### .claude/CLAUDE.md

**What**: Development standards, principles, and conventions specific to Fraisier
**When to read**: Before writing code
**Length**: ~400 lines
**Content**: Architecture principles, testing strategy, common tasks, code patterns, debugging

### ARCHITECTURE.md

**What**: Technical architecture, component overview, data flow
**When to read**: Understanding how code is organized
**Length**: ~500 lines
**Content**: High-level diagram, component details, data flows, design patterns, extension points

### DEPLOYMENT_GUIDE.md

**What**: How to deploy Fraisier and manage services in production
**When to read**: Before deploying to production
**Length**: ~600 lines
**Content**: Prerequisites, installation, configuration, systemd setup, webhooks, monitoring, troubleshooting

### TESTING.md

**What**: Testing strategy, examples, and best practices
**When to read**: Before writing tests
**Length**: ~400 lines
**Content**: Test structure, how to run tests, unit/integration/E2E examples, mocking, coverage

### PRD.md (This directory)

**What**: Complete product requirements and specifications
**When to read**: Understanding full scope and features
**Length**: ~1,300 lines
**Content**: Vision, vocabulary, architecture patterns, configuration schema, GraphQL API, roadmap

---

## Quick Reference Searches

### How do I

| Question | Find Answer In |
|----------|---|
| Set up Fraisier for development? | [../DEVELOPMENT.md](../DEVELOPMENT.md) |
| Write a test? | [TESTING.md](TESTING.md) |
| Deploy to production? | [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md) |
| Understand the code? | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Add a new CLI command? | [../.claude/CLAUDE.md](../.claude/CLAUDE.md#adding-a-new-cli-command) |
| Add a new deployer? | [../.claude/CLAUDE.md](../.claude/CLAUDE.md#adding-a-new-deployer-type) |
| Add a new Git provider? | [../.claude/CLAUDE.md](../.claude/CLAUDE.md#adding-a-new-git-provider) |
| Set up webhooks? | [DEPLOYMENT_GUIDE.md#git-webhook-configuration](DEPLOYMENT_GUIDE.md#git-webhook-configuration) |
| Debug deployment failures? | [DEPLOYMENT_GUIDE.md#troubleshooting](DEPLOYMENT_GUIDE.md#troubleshooting) |
| See the project roadmap? | [../ROADMAP.md](../ROADMAP.md) |

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
| DEVELOPMENT.md | 2026-01-22 | 0.1.0 |
| ROADMAP.md | 2026-01-22 | 0.1.0 |
| .claude/CLAUDE.md | 2026-01-22 | 0.1.0 |
| ARCHITECTURE.md | 2026-01-22 | 0.1.0 |
| DEPLOYMENT_GUIDE.md | 2026-01-22 | 0.1.0 |
| TESTING.md | 2026-01-22 | 0.1.0 |
| PRD.md | 2026-01-15 | 1.0.0 |
| **API_REFERENCE.md** | **2026-01-22** | **0.1.0** |
| **CLI_REFERENCE.md** | **2026-01-22** | **0.1.0** |
| **WEBHOOK_REFERENCE.md** | **2026-01-22** | **0.1.0** |
| **EVENT_REFERENCE.md** | **2026-01-22** | **0.1.0** |
| **GETTING_STARTED_SQLITE.md** | **2026-01-22** | **0.1.0** |
| **GETTING_STARTED_POSTGRES.md** | **2026-01-22** | **0.1.0** |
| **GETTING_STARTED_MYSQL.md** | **2026-01-22** | **0.1.0** |
| **GETTING_STARTED_DOCKER.md** | **2026-01-22** | **0.1.0** |
| **PROVIDER_BARE_METAL.md** | **2026-01-22** | **0.1.0** |
| **PROVIDER_DOCKER_COMPOSE.md** | **2026-01-22** | **0.1.0** |
| **PROVIDER_COOLIFY.md** | **2026-01-22** | **0.1.0** |
| **MONITORING_SETUP.md** | **2026-01-22** | **0.1.0** |
| **TROUBLESHOOTING.md** | **2026-01-22** | **0.1.0** |
| **REAL_WORLD_EXAMPLES.md** | **2026-01-22** | **0.1.0** |
| **FAQ_AND_ADVANCED_TOPICS.md** | **2026-01-22** | **0.1.0** |

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
- Keep INDEX.md synchronized with new files

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
- **Deployment questions**: See [DEPLOYMENT_GUIDE.md#troubleshooting](DEPLOYMENT_GUIDE.md#troubleshooting)
- **Design questions**: See [ARCHITECTURE.md](ARCHITECTURE.md)
- **Testing help**: See [TESTING.md](TESTING.md)

---

**Last Updated**: 2026-01-22
**Maintained by**: FraiseQL Team
