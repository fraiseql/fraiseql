"""
FraiseQL Repository Pattern

Implements CQRS (Command Query Responsibility Segregation) pattern with:
- CommandRepository: Write operations to tb_* tables
- QueryRepository: Read operations from tv_* views
- Sync functions: Explicit synchronization between command and query sides

Example:
    from fraiseql.repositories import CommandRepository, QueryRepository
    from fraiseql.repositories.sync import sync_tv_user

    # Write to command side
    cmd_repo = CommandRepository(db)
    user_id = await cmd_repo.execute(
        "INSERT INTO tb_user (name) VALUES ($1) RETURNING id",
        "Alice"
    )

    # Explicit sync
    await sync_tv_user(db, user_id)

    # Read from query side
    query_repo = QueryRepository(db)
    user = await query_repo.find_one("tv_user", id=user_id)
"""

# Exports will be added as implementation progresses:
# from fraiseql.repositories.command import CommandRepository
# from fraiseql.repositories.query import QueryRepository
# from fraiseql.repositories import sync

__all__ = [
    # Add exports as they are implemented
]
