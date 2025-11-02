# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 10
# Supported operators
"eq", "neq", "in", "notin"
"isnull"

# Restricted - THROWS ERROR
"contains", "startswith", "endswith"  # Not supported due to macaddr normalization
