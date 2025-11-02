# Extracted from: docs/architecture/DIRECT_PATH_IMPLEMENTATION.md
# Block number: 1
parse_graphql_query_simple('query { user(id: "123") { id firstName } }')
# Returns:
{"field_name": "user", "arguments": {"id": "123"}, "field_paths": [["id"], ["firstName"]]}
