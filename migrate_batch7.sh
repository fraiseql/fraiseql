#!/bin/bash

opencode run -m xai/grok-code-fast-1 "
TASK: Migrate SQL test files to class_db_pool architecture (Phase 4 - Batch 7)

CONTEXT:
- Project: FraiseQL (GraphQL framework with PostgreSQL)
- Language: Python 3.10+
- Testing: pytest with pytest-asyncio
- Working directory: /home/lionel/code/fraiseql

TRANSFORMATION RULES:
1. Replace CQRSRepository(conn) with FraiseQLRepository(class_db_pool)
2. Update imports: add 'from fraiseql.db import FraiseQLRepository'
3. Remove 'async with class_db_pool.connection() as conn:' context managers
4. Remove 'conn = ...' assignments since repo works directly with pool
5. Update database setup code to use repository methods instead of direct conn.execute()
6. For files already using class_db_pool but still using CQRSRepository, complete the migration

FILES TO MIGRATE:
1. tests/integration/database/sql/test_where_clause_bug.py
2. tests/integration/database/sql/test_graphql_where_repository_fix.py

VERIFICATION:
Run these commands to verify your work:
- uv run pytest tests/integration/database/sql/test_where_clause_bug.py -v
- uv run pytest tests/integration/database/sql/test_graphql_where_repository_fix.py -v
- uv run ruff check tests/integration/database/sql/test_where_clause_bug.py
- uv run ruff check tests/integration/database/sql/test_graphql_where_repository_fix.py

ACCEPTANCE CRITERIA:
- [ ] Both files use FraiseQLRepository(class_db_pool) instead of CQRSRepository(conn)
- [ ] All imports updated correctly
- [ ] No direct connection usage in repository instantiation
- [ ] Database setup code adapted to work with repository methods
- [ ] All tests pass
- [ ] No linting errors

DO NOT:
- Modify files that are already fully migrated (like test_sql_injection_real_db.py)
- Change test logic or assertions - only migrate the database access pattern
- Add new tests or modify existing test behavior

COMPLETION SIGNAL:
When done, write your status to: /tmp/opencode-phase4-batch7.marker
- On success: echo 'SUCCESS' > /tmp/opencode-phase4-batch7.marker
- On failure: echo 'FAILURE:<reason>' > /tmp/opencode-phase4-batch7.marker
This marker file is REQUIRED - do not skip this step.
"
