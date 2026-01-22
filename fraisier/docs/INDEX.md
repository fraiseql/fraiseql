# Fraisier Documentation Index

Complete documentation for the Fraisier deployment orchestrator and reference implementation.

---

## Quick Navigation

### For Getting Started
- **New to Fraisier?** Start with [../README.md](../README.md)
- **Setting up development?** See [../DEVELOPMENT.md](../DEVELOPMENT.md)
- **Deploying to production?** See [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md)

### For Development
- **Project standards?** See [../.claude/CLAUDE.md](../.claude/CLAUDE.md)
- **What needs building?** See [../ROADMAP.md](../ROADMAP.md)
- **Writing tests?** See [TESTING.md](TESTING.md)
- **Understanding architecture?** See [ARCHITECTURE.md](ARCHITECTURE.md)

### For Operations
- **Deploying services?** See [DEPLOYMENT_GUIDE.md](DEPLOYMENT_GUIDE.md)
- **Monitoring deployments?** See [DEPLOYMENT_GUIDE.md#monitoring--logs](DEPLOYMENT_GUIDE.md#monitoring--logs)
- **Troubleshooting?** See [DEPLOYMENT_GUIDE.md#troubleshooting](DEPLOYMENT_GUIDE.md#troubleshooting)

---

## Documentation Files

### In this `docs/` Directory

| File | Purpose | Audience |
|------|---------|----------|
| **ARCHITECTURE.md** | Detailed technical architecture | Engineers, Architects |
| **DEPLOYMENT_GUIDE.md** | Production deployment instructions | DevOps, Operators |
| **TESTING.md** | Testing strategy and examples | Engineers, QA |
| **PRD.md** | Product requirements document | Product Managers, Architects |

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

### How do I...

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
