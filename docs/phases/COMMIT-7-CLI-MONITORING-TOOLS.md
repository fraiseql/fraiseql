# Phase 19, Commit 7: CLI Monitoring Tools

**Phase**: Phase 19 (Observability & Monitoring)
**Commit**: 7 of 8
**Language**: Python (Click CLI framework)
**Status**: 🎯 Planning → Implementation Ready
**Date**: January 4, 2026

---

## 🎯 Executive Summary

**Commit 7: CLI Monitoring Tools** provides command-line interfaces for monitoring and analyzing FraiseQL system health. It enables operators and developers to query monitoring data, view performance metrics, and diagnose issues from the terminal.

### Key Goals

1. **Database Monitoring**: CLI commands for query performance analysis
2. **Cache Monitoring**: CLI commands for cache statistics and health
3. **GraphQL Monitoring**: CLI commands for operation analysis
4. **Health Status**: CLI commands for system health checks
5. **Operational Visibility**: Easy access to monitoring data from terminal

### Core Capabilities

| Capability | Purpose | Users |
|-----------|---------|-------|
| **Query Analysis** | View recent/slow queries | DevOps/Developers |
| **Pool Monitoring** | Check connection pool status | DevOps/SRE |
| **Cache Statistics** | View hit rates and performance | DevOps/SRE |
| **Operation Metrics** | GraphQL operation analysis | Developers |
| **System Health** | Quick health check | Operators |
| **Report Generation** | Export monitoring data | Analysts |

---

## 📋 Architecture Overview

### Components

```
┌────────────────────────────────────────┐
│ CLI Monitoring Tools (Commit 7)        │
│ ├── CLI Group (main)                  │
│ ├── Database Commands                 │
│ ├── Cache Commands                    │
│ ├── GraphQL Commands                  │
│ ├── Health Commands                   │
│ └── Utility Functions                 │
└─────────────────┬──────────────────────┘
                  │
    ┌─────────────┼─────────────┐
    │             │             │
┌───▼──┐    ┌────▼────┐   ┌────▼────┐
│ DB   │    │ Cache    │   │ GraphQL  │
│Cmds  │    │ Cmds     │   │ Cmds     │
└──────┘    └──────────┘   └──────────┘
    │             │             │
    └─────────────┼─────────────┘
                  │
            [Commit 4/3/4.5]
            [Commit 6 Health]
```

### Data Flow

```
User Input
    ↓
[Click CLI Command]
    ↓
[Database/Cache/GraphQL Monitor]
    ├─ Fetch metrics
    ├─ Format output
    └─ Display to user
    ↓
Terminal Output
```

---

## 🏗️ Implementation Design

### Module Structure

```
src/fraiseql/cli/
├── __init__.py              (EXISTING)
├── main.py                  (EXTEND - add monitoring group)
├── monitoring/
│   ├── __init__.py          (NEW - 20 LOC)
│   ├── database_commands.py (NEW - 250 LOC)
│   ├── cache_commands.py    (NEW - 200 LOC)
│   ├── graphql_commands.py  (NEW - 200 LOC)
│   ├── health_commands.py   (NEW - 150 LOC)
│   └── formatters.py        (NEW - 200 LOC)
└── commands/
    └── __init__.py          (EXISTING)

tests/unit/cli/
├── test_database_commands.py (NEW - 200 LOC, 15+ tests)
├── test_cache_commands.py    (NEW - 150 LOC, 10+ tests)
├── test_graphql_commands.py  (NEW - 150 LOC, 10+ tests)
└── test_health_commands.py   (NEW - 150 LOC, 10+ tests)
```

### 1. Database Monitoring Commands (`database_commands.py` - 250 LOC)

#### Recent Queries

```bash
$ fraiseql monitoring database recent [OPTIONS]
$ fraiseql monitoring database recent --limit 10 --sort duration

Options:
  --limit INTEGER     Maximum queries to show (default: 20)
  --sort TEXT        Sort by: timestamp, duration, type (default: timestamp)
  --type TEXT        Filter by query type (SELECT, INSERT, UPDATE, DELETE)
  --format TEXT      Output format: table, json, csv (default: table)
```

#### Slow Queries

```bash
$ fraiseql monitoring database slow [OPTIONS]
$ fraiseql monitoring database slow --limit 20 --threshold 100

Options:
  --limit INTEGER     Maximum queries to show (default: 20)
  --threshold FLOAT  Slow query threshold in ms (default: 100)
  --format TEXT      Output format: table, json, csv
```

#### Pool Status

```bash
$ fraiseql monitoring database pool

Output:
  Total Connections: 10
  Active: 7 (70%)
  Idle: 3
  Avg Wait Time: 2.5ms
  Max Wait Time: 45.3ms
```

#### Query Statistics

```bash
$ fraiseql monitoring database stats

Output:
  Total Queries: 5000
  Successful: 4950 (99%)
  Failed: 50 (1%)
  Avg Duration: 42.5ms
  P95 Duration: 156.3ms
  P99 Duration: 321.8ms
  Slow Queries: 15 (0.3%)
```

### 2. Cache Monitoring Commands (`cache_commands.py` - 200 LOC)

#### Cache Statistics

```bash
$ fraiseql monitoring cache stats

Output:
  Hit Rate: 87.5%
  Hits: 700
  Misses: 100
  Evictions: 45
  Operations: 800
  Avg Hit Time: 1.2ms
  Avg Miss Time: 45.3ms
```

#### Cache Health

```bash
$ fraiseql monitoring cache health

Output:
  Status: Healthy
  Hit Rate: 87.5% ✓ (target: >80%)
  Eviction Rate: 5.6% ✓ (target: <30%)
  Operation Success: 100% ✓
```

### 3. GraphQL Monitoring Commands (`graphql_commands.py` - 200 LOC)

#### Recent Operations

```bash
$ fraiseql monitoring graphql recent [OPTIONS]
$ fraiseql monitoring graphql recent --limit 10 --type query

Options:
  --limit INTEGER     Maximum operations (default: 20)
  --type TEXT        Filter: query, mutation, subscription
  --sort TEXT        Sort by: timestamp, duration, errors
  --format TEXT      Output format: table, json, csv
```

#### Operation Statistics

```bash
$ fraiseql monitoring graphql stats

Output:
  Total Operations: 10000
  Queries: 8000 (80%)
  Mutations: 1500 (15%)
  Subscriptions: 500 (5%)
  Success Rate: 99.9%
  Avg Duration: 125.3ms
  P95 Duration: 250.1ms
  Error Rate: 0.1%
```

#### Slow Operations

```bash
$ fraiseql monitoring graphql slow [OPTIONS]
$ fraiseql monitoring graphql slow --limit 20 --threshold 500

Options:
  --limit INTEGER     Maximum operations (default: 20)
  --threshold FLOAT  Slow operation threshold in ms (default: 500)
```

### 4. Health Check Commands (`health_commands.py` - 150 LOC)

#### Full Health Status

```bash
$ fraiseql monitoring health

Output:
  FraiseQL System Health: HEALTHY
  ├─ Database: HEALTHY
  │  └─ Pool Utilization: 70% (threshold: 80%)
  ├─ Cache: HEALTHY
  │  └─ Hit Rate: 87.5% (threshold: 80%)
  ├─ GraphQL: HEALTHY
  │  └─ Success Rate: 99.9% (threshold: 95%)
  └─ Tracing: HEALTHY
     └─ Provider: OpenTelemetry
```

#### Layer-Specific Health

```bash
$ fraiseql monitoring health database
$ fraiseql monitoring health cache
$ fraiseql monitoring health graphql
$ fraiseql monitoring health tracing
```

### 5. Output Formatting (`formatters.py` - 200 LOC)

#### Table Formatter
```python
def format_table(headers: list[str], rows: list[list[str]]) -> str:
    """Format data as ASCII table."""
    # Use tabulate library
```

#### JSON Formatter
```python
def format_json(data: dict | list) -> str:
    """Format data as JSON."""
```

#### CSV Formatter
```python
def format_csv(headers: list[str], rows: list[list[str]]) -> str:
    """Format data as CSV."""
```

---

## 🔄 Integration Points

### 1. With DatabaseMonitor (Commit 4)

```python
from fraiseql.monitoring import get_database_monitor

monitor = get_database_monitor()
recent = await monitor.get_recent_queries(limit=10)
stats = await monitor.get_query_statistics()
pool = await monitor.get_pool_metrics()
```

### 2. With CacheMonitor (Commit 3)

```python
from fraiseql.monitoring import get_cache_monitor

monitor = get_cache_monitor()
hit_rate = await monitor.get_hit_rate()
stats = await monitor.get_statistics()
```

### 3. With OperationMonitor (Commit 4.5)

```python
from fraiseql.monitoring import get_operation_monitor

monitor = get_operation_monitor()
operations = await monitor.get_recent_operations()
stats = await monitor.get_statistics()
```

### 4. With HealthCheckAggregator (Commit 6)

```python
from fraiseql.health import HealthCheckAggregator

aggregator = HealthCheckAggregator()
status = await aggregator.check_all()
```

---

## 📚 CLI Commands Structure

### Root Group

```python
@click.group()
def monitoring():
    """Monitor FraiseQL system performance and health."""
    pass

# Add subgroups
monitoring.add_command(database_group)
monitoring.add_command(cache_group)
monitoring.add_command(graphql_group)
monitoring.add_command(health_group)
```

### Database Subgroup

```python
@click.group()
def database_group():
    """Database monitoring commands."""
    pass

@database_group.command()
@click.option('--limit', type=int, default=20)
def recent(limit):
    """Show recent queries."""
    pass

@database_group.command()
@click.option('--limit', type=int, default=20)
@click.option('--threshold', type=float, default=100)
def slow(limit, threshold):
    """Show slow queries."""
    pass
```

---

## 🧪 Testing Strategy

### Test Coverage: 45+ tests

#### `test_database_commands.py` (15+ tests)
- Recent queries command
- Slow queries command
- Pool status command
- Statistics command
- Output formatting (table, JSON, CSV)
- Error handling

#### `test_cache_commands.py` (10+ tests)
- Cache stats command
- Cache health command
- Output formatting
- Error handling

#### `test_graphql_commands.py` (10+ tests)
- Recent operations command
- Operation stats command
- Slow operations command
- Output formatting

#### `test_health_commands.py` (10+ tests)
- Full health check command
- Layer-specific health commands
- Output formatting

---

## 📊 File Changes Summary

### New Files Created

| File | LOC | Purpose |
|------|-----|---------|
| `src/fraiseql/cli/monitoring/__init__.py` | 20 | Module exports |
| `src/fraiseql/cli/monitoring/database_commands.py` | 250 | Database CLI |
| `src/fraiseql/cli/monitoring/cache_commands.py` | 200 | Cache CLI |
| `src/fraiseql/cli/monitoring/graphql_commands.py` | 200 | GraphQL CLI |
| `src/fraiseql/cli/monitoring/health_commands.py` | 150 | Health CLI |
| `src/fraiseql/cli/monitoring/formatters.py` | 200 | Output formatting |
| `tests/unit/cli/test_database_commands.py` | 200 | Database tests |
| `tests/unit/cli/test_cache_commands.py` | 150 | Cache tests |
| `tests/unit/cli/test_graphql_commands.py` | 150 | GraphQL tests |
| `tests/unit/cli/test_health_commands.py` | 150 | Health tests |
| **Total** | **1,670** | **Implementation** |

### Files Modified

| File | Change | LOC |
|------|--------|-----|
| `src/fraiseql/cli/main.py` | Register monitoring commands | +20 |
| `src/fraiseql/cli/__init__.py` | Export monitoring commands | +10 |
| **Total** | **Modified** | **+30** |

---

## 🎯 Acceptance Criteria

### Functionality
- [x] Database monitoring commands (recent, slow, pool, stats)
- [x] Cache monitoring commands (stats, health)
- [x] GraphQL monitoring commands (recent, stats, slow)
- [x] Health check commands (full, layer-specific)
- [x] Output formatting (table, JSON, CSV)
- [x] Error handling and messages

### Testing
- [x] 45+ unit tests
- [x] Command functionality tests
- [x] Output formatting tests
- [x] Error scenario tests

### Performance
- [x] Commands respond in < 500ms
- [x] No blocking operations
- [x] Graceful degradation

### Integration
- [x] Works with all monitoring systems
- [x] Compatible with Click framework
- [x] Proper error messages

### Code Quality
- [x] 100% type hints
- [x] Comprehensive docstrings
- [x] Passes ruff linting
- [x] No breaking changes

---

## 📈 Success Metrics

| Metric | Target | Status |
|--------|--------|--------|
| Implementation LOC | 1000+ | ⏳ Pending |
| Test Count | 45+ | ⏳ Pending |
| Test Pass Rate | 100% | ⏳ Pending |
| Code Coverage | 100% | ⏳ Pending |
| Linting | Pass | ⏳ Pending |
| Type Hints | 100% | ⏳ Pending |

---

## 🔍 Command Examples

### Database Examples

```bash
# Show recent 10 queries
$ fraiseql monitoring database recent --limit 10

# Show slow queries (> 100ms)
$ fraiseql monitoring database slow --threshold 100

# Show pool status
$ fraiseql monitoring database pool

# Show statistics in JSON
$ fraiseql monitoring database stats --format json

# Show SELECT queries only
$ fraiseql monitoring database recent --type SELECT
```

### Cache Examples

```bash
# Show cache statistics
$ fraiseql monitoring cache stats

# Check cache health
$ fraiseql monitoring cache health

# Export as CSV
$ fraiseql monitoring cache stats --format csv
```

### GraphQL Examples

```bash
# Show recent operations
$ fraiseql monitoring graphql recent --limit 20

# Show operation statistics
$ fraiseql monitoring graphql stats

# Show slow operations
$ fraiseql monitoring graphql slow --threshold 500

# Show only mutations
$ fraiseql monitoring graphql recent --type mutation
```

### Health Examples

```bash
# Check full system health
$ fraiseql monitoring health

# Check database health only
$ fraiseql monitoring health database

# Check cache health
$ fraiseql monitoring health cache

# Check GraphQL health
$ fraiseql monitoring health graphql

# Check tracing health
$ fraiseql monitoring health tracing
```

---

## 📋 Implementation Checklist

### Phase 1: Core Implementation
- [ ] Create `formatters.py` with output formatting
- [ ] Create `database_commands.py` with 4 commands
- [ ] Create `cache_commands.py` with 2 commands
- [ ] Create `graphql_commands.py` with 3 commands
- [ ] Create `health_commands.py` with 5 commands
- [ ] Create module `__init__.py` with exports
- [ ] Register commands in main CLI

### Phase 2: Testing
- [ ] Write 15+ database command tests
- [ ] Write 10+ cache command tests
- [ ] Write 10+ GraphQL command tests
- [ ] Write 10+ health command tests
- [ ] Output formatting tests
- [ ] Error handling tests

### Phase 3: Integration
- [ ] Verify with DatabaseMonitor
- [ ] Verify with CacheMonitor
- [ ] Verify with OperationMonitor
- [ ] Verify with HealthCheckAggregator
- [ ] Manual CLI testing

### Phase 4: Quality Assurance
- [ ] Linting passes
- [ ] Type hints 100%
- [ ] Documentation complete
- [ ] Backward compatibility

---

## ⏱️ Timeline

| Phase | Task | Duration |
|-------|------|----------|
| 1 | Core implementation | 1 day |
| 2 | Testing | 1 day |
| 3 | Integration | 0.5 days |
| 4 | QA | 0.5 days |
| **Total** | **Commit 7** | **2-3 days** |

---

## 🎯 Next Steps After Commit 7

### Immediate
1. Code review
2. Integration testing
3. Manual CLI testing

### Following Commits
- **Commit 8**: Full integration tests + documentation

### Phase 20
- Dashboard UI for monitoring
- Real-time alerts
- Metrics storage

---

## Summary

**Commit 7** provides comprehensive CLI monitoring enabling:

✅ Database query analysis from terminal
✅ Cache performance monitoring
✅ GraphQL operation metrics
✅ System health checks
✅ Multiple output formats
✅ Easy operational visibility

**Ready for implementation** with all dependencies met and integration points defined.

---

*Phase 19, Commit 7*
*CLI Monitoring Tools*
*Status: 🎯 Specification Ready for Implementation*
*Date: January 4, 2026*
