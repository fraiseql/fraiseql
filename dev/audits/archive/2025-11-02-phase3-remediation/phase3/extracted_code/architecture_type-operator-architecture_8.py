# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 8
# Basic operators
"eq", "neq", "in", "notin"

# Hierarchical relationships
"ancestor_of"  # path1 @> path2 (ancestor contains descendant)
"descendant_of"  # path1 <@ path2 (descendant is contained)

# Pattern matching
"matches_lquery"  # path ~ lquery (wildcard patterns)
"matches_ltxtquery"  # path ? ltxtquery (text queries)

# Restricted
"contains", "startswith", "endswith"  # THROWS ERROR - not valid for ltree
