# Extracted from: docs/core/concepts-glossary.md
# Block number: 11
import fraiseql
from fraiseql.types.generic import Connection


@fraiseql.connection(node_type=User, default_page_size=20, max_page_size=100)
async def users(info, first: int | None = None, after: str | None = None) -> Connection[User]:
    """Get paginated users - pagination handled automatically."""
    # Framework calls db.paginate() automatically
    # Returns Connection with nodes, pageInfo, totalCount
