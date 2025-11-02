# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 9
# Basic operators
"eq", "neq", "in", "notin"

# Range relationships
"contains_date"  # range @> date
"overlaps"  # range1 && range2
"adjacent"  # range1 -|- range2
"strictly_left"  # range1 << range2
"strictly_right"  # range1 >> range2
"not_left"  # range1 &> range2
"not_right"  # range1 &< range2

# Restricted
"contains", "startswith", "endswith"  # THROWS ERROR - not valid for daterange
