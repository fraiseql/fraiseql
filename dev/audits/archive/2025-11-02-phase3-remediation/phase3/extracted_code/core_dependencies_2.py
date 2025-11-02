# Extracted from: docs/core/dependencies.md
# Block number: 2
from fraiseql.caching import setup_auto_cascade_rules

await setup_auto_cascade_rules(cache, schema, verbose=True)
# CASCADE: Detected relationship: User -> Post
# CASCADE: Created 3 CASCADE rules
