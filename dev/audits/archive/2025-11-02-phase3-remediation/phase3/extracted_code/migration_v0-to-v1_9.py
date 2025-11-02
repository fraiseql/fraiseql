# Extracted from: docs/migration/v0-to-v1.md
# Block number: 9
from fraiseql import query


# JSON responses are now 10-100x faster
@query
def get_data(info: Info) -> dict:
    return {"key": "value"}  # Fast JSON serialization
