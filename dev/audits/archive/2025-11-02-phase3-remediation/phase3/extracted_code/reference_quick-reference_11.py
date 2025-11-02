# Extracted from: docs/reference/quick-reference.md
# Block number: 11
# Basic
"eq", "neq", "in", "notin"

# Hierarchical
("ancestor_of",)  # path1 @> path2
("descendant_of",)  # path1 <@ path2

# Pattern matching
("matches_lquery",)  # path ~ lquery
"matches_ltxtquery"  # path ? ltxtquery

# RESTRICTED (throws error)
"contains", "startswith", "endswith"
