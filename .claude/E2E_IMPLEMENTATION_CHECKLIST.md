# E2E Testing Implementation Checklist

**Timeline**: 2-3 days to complete
**Effort**: ~12-14 hours
**Status**: Ready to implement

---

## Phase 1: Create E2E Test Files (4 hours)

### Python E2E Test
- [ ] Create `tests/e2e/python_e2e_test.py`
  - [ ] test_python_e2e_basic_schema()
  - [ ] test_python_e2e_analytics_schema()
  - [ ] Validate JSON output
  - [ ] Test CLI compilation
- [ ] Add pytest configuration if needed
- [ ] Run locally: `pytest tests/e2e/python_e2e_test.py -v`

### TypeScript E2E Test
- [ ] Create `fraiseql-typescript/tests/e2e/e2e.test.ts`
  - [ ] should author basic schema
  - [ ] should export schema to JSON
  - [ ] should compile with CLI
  - [ ] should validate responses
- [ ] Add Jest E2E test suite
- [ ] Run locally: `npm run test:e2e`

### Java E2E Test
- [ ] Create `fraiseql-java/src/test/java/com/fraiseql/E2ETest.java`
  - [ ] testBasicSchemaAuthoring()
  - [ ] testCliCompilation()
  - [ ] testRuntimeExecution()
  - [ ] testAnalyticsSchema()
- [ ] Add to pom.xml Maven configuration
- [ ] Run locally: `mvn test -Dtest="*E2ETest"`

### Go E2E Test
- [ ] Create `fraiseql-go/fraiseql/e2e_test.go`
  - [ ] TestE2EBasicSchema()
  - [ ] TestE2EAnalyticsSchema()
  - [ ] TestE2ECliCompilation()
  - [ ] TestE2ERuntime()
- [ ] Run locally: `go test ./fraiseql/... -run TestE2E -v`

### PHP E2E Test
- [ ] Create `fraiseql-php/tests/e2e/E2ETest.php`
  - [ ] testBasicSchemaAuthoring()
  - [ ] testJsonExport()
  - [ ] testCliCompilation()
  - [ ] testAnalyticsSchema()
- [ ] Add to phpunit.xml configuration
- [ ] Run locally: `vendor/bin/phpunit tests/e2e/`

---

## Phase 2: Implement Makefile Targets (2 hours)

### Update Main Makefile
- [ ] Add `e2e-setup` target
  - [ ] Start Docker containers
  - [ ] Wait for databases
  - [ ] Verify connectivity
- [ ] Add `e2e-all` target (runs all 5 languages sequentially)
- [ ] Add `e2e-python` target
  - [ ] Create venv
  - [ ] Install dependencies
  - [ ] Run tests
- [ ] Add `e2e-typescript` target
  - [ ] npm ci
  - [ ] Run tests
- [ ] Add `e2e-java` target
  - [ ] Maven dependency download
  - [ ] Run tests
- [ ] Add `e2e-go` target
  - [ ] Go module download
  - [ ] Run tests
- [ ] Add `e2e-php` target
  - [ ] Composer install
  - [ ] Run tests
- [ ] Add `e2e-clean` target
  - [ ] Stop Docker containers
  - [ ] Remove temp files
  - [ ] Cleanup volumes
- [ ] Add `e2e-status` target
  - [ ] Show Docker status
  - [ ] Test database connectivity

### Test Makefile Locally
- [ ] `make e2e-setup` - Start infrastructure
- [ ] `make e2e-status` - Verify databases ready
- [ ] `make e2e-python` - Test Python pipeline
- [ ] `make e2e-typescript` - Test TypeScript pipeline
- [ ] `make e2e-java` - Test Java pipeline
- [ ] `make e2e-go` - Test Go pipeline
- [ ] `make e2e-php` - Test PHP pipeline
- [ ] `make e2e-clean` - Stop and cleanup

---

## Phase 3: GitHub Actions CI/CD Setup (3 hours)

### Create Workflow File
- [ ] Create `.github/workflows/e2e-tests.yml`
  - [ ] Set up Python environment and cache
  - [ ] Set up Node environment and cache
  - [ ] Set up Java environment and cache
  - [ ] Set up Go environment and cache
  - [ ] Set up PHP environment and cache

### Configure Services
- [ ] PostgreSQL 16 service
  - [ ] Health check configuration
  - [ ] Port mapping (5432)
  - [ ] Environment variables
- [ ] MySQL 8.3 service
  - [ ] Health check configuration
  - [ ] Port mapping (3306)
  - [ ] Environment variables

### Create Job Matrix
- [ ] Python test job
  - [ ] Dependencies installation
  - [ ] Test execution
  - [ ] Artifact upload
- [ ] TypeScript test job
  - [ ] npm ci
  - [ ] npm test:e2e
  - [ ] Artifact upload
- [ ] Java test job
  - [ ] Maven test
  - [ ] Report generation
  - [ ] Artifact upload
- [ ] Go test job
  - [ ] go test
  - [ ] Coverage report
  - [ ] Artifact upload
- [ ] PHP test job
  - [ ] Composer install
  - [ ] PHPUnit execution
  - [ ] Artifact upload
- [ ] CLI integration job
  - [ ] Runs after all language tests
  - [ ] Build fraiseql-cli
  - [ ] Test with generated schemas

### Add Workflow Features
- [ ] Trigger on push to main/develop
- [ ] Trigger on pull requests
- [ ] Scheduled daily run (optional)
- [ ] Matrix strategy for parallelization
- [ ] Cache configuration for dependencies
- [ ] Artifact uploads
- [ ] Summary report generation
- [ ] Failure notifications

### Test Workflow
- [ ] Push to feature branch
- [ ] Verify workflow runs
- [ ] Check all jobs execute
- [ ] Verify caching works
- [ ] Check artifact uploads
- [ ] Review summary report

---

## Phase 4: CLI Schema Format Resolution (2-4 hours)

### Investigation
- [ ] [ ] Review fraiseql-cli schema parser code
  - [ ] Look at `fraiseql-cli/src/compile.rs`
  - [ ] Check schema validation logic
  - [ ] Find schema format documentation

- [ ] [ ] Compare generated schemas
  - [ ] Go-generated schema.json
  - [ ] Expected CLI input format
  - [ ] Output format (schema.compiled.json)

- [ ] [ ] Identify gaps
  - [ ] Missing fields?
  - [ ] Wrong structure?
  - [ ] Type mismatches?
  - [ ] Metadata issues?

### Fix Strategy
Choose one approach:

**Option A: Fix Generators** (if CLI format is correct)
- [ ] Update Python generator
- [ ] Update TypeScript generator
- [ ] Update Java generator
- [ ] Update Go generator
- [ ] Update PHP generator

**Option B: Fix CLI** (if generator format is correct)
- [ ] Update CLI schema parser
- [ ] Update validation logic
- [ ] Document schema format expectations
- [ ] Add format conversion if needed

**Option C: Add Transformation Layer**
- [ ] Create schema transformer
- [ ] Map between formats
- [ ] Update E2E tests to use transformer

### Verification
- [ ] [ ] Test with Go schema: `fraiseql-cli compile schema.json`
- [ ] [ ] Verify compilation succeeds
- [ ] [ ] Check schema.compiled.json output
- [ ] [ ] Test with Python-generated schema
- [ ] [ ] Test with TypeScript-generated schema
- [ ] [ ] Test with Java-generated schema
- [ ] [ ] Test with PHP-generated schema

---

## Phase 5: Full E2E Pipeline Testing (1 hour)

### Local Execution
- [ ] Start all databases: `make e2e-setup`
- [ ] Run all tests: `make e2e-all`
- [ ] Monitor progress
- [ ] Collect results
- [ ] Fix any failures

### Verification Checklist
- [ ] Python: 7+ tests passing
- [ ] TypeScript: 10+ tests passing
- [ ] Java: 82+ tests passing
- [ ] Go: 45+ tests passing
- [ ] PHP: 40+ tests passing

### Report Generation
- [ ] [ ] Collect test results
- [ ] [ ] Generate coverage report
- [ ] [ ] Document any issues
- [ ] [ ] Create summary report

### Cleanup
- [ ] `make e2e-clean` to stop infrastructure
- [ ] Verify volumes removed
- [ ] Verify temp files cleaned

---

## Post-Implementation Tasks

### Documentation
- [ ] [ ] Update main README.md with E2E testing section
- [ ] [ ] Document test running in CONTRIBUTING.md
- [ ] [ ] Create E2E testing guide
- [ ] [ ] Document CI/CD pipeline in DEVELOPMENT.md

### Monitoring
- [ ] [ ] Set up GitHub Actions notifications
- [ ] [ ] Configure Slack/email alerts for failures
- [ ] [ ] Create dashboard for test results

### Maintenance
- [ ] [ ] Schedule weekly E2E test runs
- [ ] [ ] Monitor test flakiness
- [ ] [ ] Update tests as new features added
- [ ] [ ] Keep Docker images updated

---

## Success Criteria

### Phase 1 Complete
- ✅ All 5 language E2E test files created
- ✅ Tests locally runnable
- ✅ No compilation errors
- ✅ Docstrings and comments complete

### Phase 2 Complete
- ✅ Makefile targets created
- ✅ `make e2e-all` works locally
- ✅ All virtual environments install correctly
- ✅ Test results clear and understandable

### Phase 3 Complete
- ✅ GitHub Actions workflow created
- ✅ Workflow runs on push/PR
- ✅ All jobs execute successfully
- ✅ Summary report generated

### Phase 4 Complete
- ✅ CLI schema format issue understood
- ✅ Fix implemented (generator or CLI)
- ✅ All 5 languages compile successfully
- ✅ schema.compiled.json generated

### Phase 5 Complete
- ✅ All E2E tests passing locally
- ✅ CI/CD pipeline fully automated
- ✅ Coverage at 90%+ for each language
- ✅ Documentation complete

---

## Estimated Timeline

| Phase | Task | Effort | Timeline |
|-------|------|--------|----------|
| 1 | E2E test files | 4 hours | Day 1 (morning) |
| 2 | Makefile targets | 2 hours | Day 1 (afternoon) |
| 3 | GitHub Actions | 3 hours | Day 2 (morning) |
| 4 | CLI format fix | 2-4 hours | Day 2 (afternoon) |
| 5 | Full pipeline test | 1 hour | Day 3 (morning) |
| - | **Total** | **12-14 hours** | **2-3 days** |

---

## Resource Requirements

### Local Development
- Docker (already available)
- 8+ GB RAM for Docker services
- 10+ GB disk space for volumes
- Python 3.10+, Node 18+, Java 17, Go 1.22, PHP 8.2+

### CI/CD
- GitHub Actions (free tier sufficient)
- ~30 minutes per full pipeline run
- ~$0 cost (GitHub Actions free tier: 2000 minutes/month)

### Databases
- PostgreSQL 16
- MySQL 8.3
- SQLite (local)
- pgvector extension (optional)

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| CLI format incompatibility | Investigate early (Phase 4) |
| Virtual environment conflicts | Use isolated venv/npm/composer |
| Docker service timing issues | Use health checks, add retries |
| Test flakiness | Use deterministic data, add seeds |
| Long pipeline times | Parallelize in CI/CD where possible |
| Cross-platform issues | Test on Linux first, then macOS/Windows |

---

## Dependencies

- ✅ Docker infrastructure already exists
- ✅ Language generators implemented
- ✅ fraiseql-cli available for testing
- ⚠️ CLI schema format needs clarification
- ⚠️ Python/TypeScript fixes needed (from earlier checklist)

---

## Notes

1. **Docker Compose Already Ready**: The docker-compose.test.yml is fully configured with PostgreSQL, MySQL, and pgvector support
2. **Language Tests Are Language-Idiomatic**: Each language uses its native testing framework (pytest, Jest, JUnit, go test, PHPUnit)
3. **Parallel Execution**: GitHub Actions can run all 5 language tests in parallel (no dependencies between them)
4. **CLI Integration Blocker**: All 5 languages blocked on CLI schema format - this must be resolved in Phase 4

---

**Document Version**: 1.0
**Created**: January 16, 2026
**Status**: Ready for implementation
**Next Action**: Start with Phase 1 (E2E test file creation)
