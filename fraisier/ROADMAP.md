# Fraisier Roadmap

**Fraisier** is the deployment orchestrator and reference implementation for the FraiseQL ecosystem.

**Current Status**: v0.1.0-alpha
**Parent Framework**: FraiseQL (v2.0.0-a1 in crates/)

---

## Vision

> A deployment orchestrator that manages any service, to any target, with any Git provider.

Fraisier:

- ✅ Demonstrates FraiseQL best practices
- ✅ Proves the framework works end-to-end
- ✅ Provides deployment orchestration for the ecosystem
- ✅ Serves as the reference for implementing FraiseQL applications

---

## Phase 1: Foundation & Core Deployment (v0.1.0)

**Timeline**: 1-2 weeks
**Status**: **IN PROGRESS** (core scaffolding done, execution needed)

### Goals

- [ ] Complete all deployer implementations (API, ETL, Scheduled)
- [ ] Write comprehensive test suite (100% coverage)
- [ ] Fix webhook handler
- [ ] Achieve deployable MVP

### Tasks

#### 1.1 Complete Deployer Implementations

**APIDeployer** (currently 40% done):

- [ ] Implement `execute()` fully (git pull, systemctl restart, health check)
- [ ] Implement `rollback()` with version tracking
- [ ] Add pre-flight checks (binary exists, systemd service active)
- [ ] Handle database migrations
- [ ] Test with mock systemd

**ETLDeployer** (currently 30% done):

- [ ] Implement execution (git pull, run scripts)
- [ ] Handle script output/logging
- [ ] Track success/failure

**ScheduledDeployer** (currently 30% done):

- [ ] Implement cron job scheduling
- [ ] Handle job dependencies
- [ ] Track execution history

**Status**: 🔴 Critical blocker

#### 1.2 Test Suite

**Unit Tests** (50+ tests):

- [ ] Config loading and validation
- [ ] Each deployer type
- [ ] Git provider signature verification (GitHub, GitLab, Gitea, Bitbucket)
- [ ] Database operations (CRUD on all tables)
- [ ] CLI commands

**Integration Tests** (20+ tests):

- [ ] Full deployment flow with database recording
- [ ] Health check validation
- [ ] Webhook event routing

**E2E Tests** (10+ tests):

- [ ] Complete CLI workflow
- [ ] Webhook-triggered deployment
- [ ] Deployment history tracking

**Status**: 🔴 Critical blocker (currently 0 tests)

#### 1.3 Webhook Handler

**Current State**: 5% complete
**Needs**:

- [ ] Complete `execute_deployment()` function
- [ ] Proper event-to-deployment routing
- [ ] Error handling and recovery
- [ ] Background job processing

**Status**: 🔴 Critical blocker

#### 1.4 Documentation

- [ ] Create `docs/` directory
- [ ] Move/organize existing documentation
- [ ] Add deployment guide
- [ ] Add troubleshooting guide
- [ ] Update main README with clear quick-start

**Status**: 🟡 Medium priority

#### 1.5 Quality Assurance

- [ ] Ruff linting passes
- [ ] Type checking passes
- [ ] 100% test coverage
- [ ] CI/CD pipeline works
- [ ] Docker build works

**Status**: 🔴 Blocker

---

## Phase 2: Deployment Providers (v0.2.0)

**Timeline**: 1-2 weeks after Phase 1
**Status**: Not started

### Goals

- [ ] Implement 3 core deployment providers
- [ ] Demonstrate provider abstraction working
- [ ] Production-ready for self-hosted deployments

### Tasks

#### 2.1 Bare Metal Provider

- [ ] SSH/systemd integration
- [ ] Service restart
- [ ] Health checks
- [ ] Log retrieval
- [ ] Tests

#### 2.2 Docker Compose Provider

- [ ] docker-compose up/down
- [ ] Service management
- [ ] Port mapping
- [ ] Volume handling
- [ ] Tests

#### 2.3 Coolify Provider

- [ ] Coolify API integration
- [ ] Project/service management
- [ ] Deployment triggering
- [ ] Status polling
- [ ] Tests

### Additional Features

- [ ] Deployment locks (prevent concurrent deploys)
- [ ] Pre-flight checks per provider
- [ ] Rollback support

---

## Phase 3: Production Hardening (v1.0.0)

**Timeline**: 2-3 weeks after Phase 2
**Status**: Not started

### Goals

- [ ] Production-ready reliability
- [ ] Comprehensive error handling
- [ ] Full monitoring/observability
- [ ] Multi-database support

### Tasks

#### 3.1 Error Handling

- [ ] Custom exception hierarchy
- [ ] Graceful failure modes
- [ ] Automatic recovery strategies
- [ ] Detailed error messages

#### 3.2 Monitoring & Observability

- [ ] Structured logging (JSON format)
- [ ] Prometheus metrics
- [ ] Distributed tracing
- [ ] Deployment dashboards

#### 3.3 Additional Providers

- [ ] AWS ECS/EC2
- [ ] Scaleway
- [ ] OVH
- [ ] Kubernetes

#### 3.4 Multi-Database Support

- [ ] PostgreSQL (primary)
- [ ] MySQL support
- [ ] SQL Server support
- [ ] Migration utilities

#### 3.5 Advanced Features

- [ ] Custom Git provider plugin system
- [ ] Notification integrations (Slack, Discord, PagerDuty)
- [ ] Deployment approvals/gates
- [ ] Canary deployments

---

## Phase 4: Multi-Language & Cloud (v1.1.0+)

**Timeline**: 3-4 weeks after Phase 3
**Status**: Future

### Goals

- [ ] Multi-language implementations
- [ ] FraiseQL Cloud platform
- [ ] Advanced orchestration features

### Tasks

#### 4.1 Language Implementations

- [ ] fraisier-typescript (TypeScript/Node.js)
- [ ] fraisier-go (Go)
- [ ] fraisier-rust (Rust)

#### 4.2 Cross-Language E2E Tests

- [ ] Python Fraisier deploys TypeScript Fraisier
- [ ] All implementations produce identical results
- [ ] Version compatibility verified

#### 4.3 FraiseQL Cloud

- [ ] Hosted Fraisier instance
- [ ] SaaS deployment platform
- [ ] Managed infrastructure

#### 4.4 Advanced Orchestration

- [ ] Multi-region deployments
- [ ] Blue-green deployments
- [ ] Traffic shifting strategies
- [ ] Automatic rollback on health check failure

---

## Version History

### v0.1.0 (Alpha) - Current

- Git provider abstraction (GitHub, GitLab, Gitea, Bitbucket)
- Configuration system (fraises.yaml)
- Database schema (CQRS pattern)
- CLI scaffolding
- Webhook server scaffolding
- **Missing**: Deployer implementations, tests, providers

### v0.2.0 (Alpha) - Planned

- Complete deployer implementations
- Test suite
- Bare metal, Docker Compose, Coolify providers
- PostgreSQL support

### v1.0.0 (Stable) - Planned

- All major deployment providers
- Production error handling
- Full monitoring
- Comprehensive documentation
- Cross-language E2E tests

### v1.1.0+ (Future)

- Multi-language implementations
- FraiseQL Cloud
- Advanced orchestration
- Enterprise features

---

## Current Blockers

| Issue | Impact | Status | Effort |
|-------|--------|--------|--------|
| Deployer implementations incomplete | Can't deploy | 🔴 CRITICAL | 2-3 days |
| No tests | Can't verify | 🔴 CRITICAL | 2-3 days |
| Webhook handler incomplete | Event-driven deploys broken | 🔴 CRITICAL | 1-2 days |
| No deployment providers | Can't reach production | 🟠 HIGH | Deferred to Phase 2 |
| Documentation scattered | Confusing | 🟡 MEDIUM | 1 day |

---

## Dependencies on FraiseQL

Fraisier depends on these FraiseQL features:

| Feature | FraiseQL Status | Fraisier Use |
|---------|-----------------|--------------|
| Schema authoring (Python) | ✅ Phase 3 | Define Fraisier's own schema |
| Query execution | ✅ Phase 5 | Execute deployment queries |
| Mutation execution | ✅ Phase 5 | Execute deployment mutations |
| File runtime | ✅ Phase 4 | Handle log files |
| Webhook runtime | ✅ Phase 4 | Process Git webhooks |
| Authentication | ✅ Phase 5 | API security |
| Event listeners | ✅ Phase 7 | Real-time deployment updates |

**All core FraiseQL features needed for Fraisier are already implemented.**

---

## Success Criteria

### Phase 1 (v0.1.0)

- ✅ CLI commands work (list, deploy, status, history, stats)
- ✅ Deployers successfully execute (with mock dependencies)
- ✅ Database records deployments accurately
- ✅ 100% test coverage
- ✅ Webhook events trigger deployments
- ✅ Ruff linting passes
- ✅ Type checking passes
- ✅ CI/CD pipeline works

### Phase 2 (v0.2.0)

- ✅ Deploy to 3 different providers
- ✅ Providers pluggable/extensible
- ✅ Deployment locks working
- ✅ Rollback functionality verified

### Phase 3 (v1.0.0)

- ✅ Production deployments tested
- ✅ Error recovery validated
- ✅ Monitoring/observability working
- ✅ Multi-database supported

### Phase 4 (v1.1.0)

- ✅ Multiple language implementations
- ✅ Cross-language E2E tests pass
- ✅ Cloud platform available

---

## Communication

### Reporting Issues

Use labels for clarity:

- `fraisier-core` - CLI, deployers, database
- `fraisier-providers` - Deployment provider implementations
- `fraisier-tests` - Testing infrastructure
- `fraisier-docs` - Documentation

### Phase Coordination

When working on Fraisier Phase X:

1. Create branch: `feature/fraisier/phase-X-<description>`
2. Update this roadmap
3. Create issue for tracking
4. Use commit prefix: `feat(fraisier): ...`

---

## Related Documents

- **Development Guide**: `.claude/CLAUDE.md` (this directory)
- **Architecture Details**: `docs/PRD.md`
- **Example Configuration**: `fraises.example.yaml`
- **Parent Framework**: `../README.md` (FraiseQL)

---

**Last Updated**: 2026-01-22
**Next Review**: After Phase 1 completion
