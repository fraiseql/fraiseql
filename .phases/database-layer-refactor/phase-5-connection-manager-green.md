# Phase 5: Connection Manager

**Phase:** GREEN (Make Tests Pass)
**Duration:** 4-6 hours
**Risk:** Medium

---

## Objective

**TDD Phase GREEN:** Extract connection pool and transaction management.

Extract:
- Pool management: `__init__()`, `get_pool()`
- Session variables: `_set_session_variables()`
- Query execution: `run()`, `run_in_transaction()`
- Connection lifecycle management

---

## Files to Create

1. `src/fraiseql/db/core/connection_manager.py` - Connection management (~250 lines)
2. `src/fraiseql/db/core/transaction.py` - Transaction handling (~150 lines)

---

## Next Phase

→ **Phase 6:** Repository Facade
