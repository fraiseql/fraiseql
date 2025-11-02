# Extracted from: docs/reference/quick-reference.md
# Block number: 12
# Basic
"eq", "neq", "in", "notin"

# Range relationships
("contains_date",)  # range @> date
("overlaps",)  # range1 && range2
("adjacent",)  # range1 -|- range2
("strictly_left",)  # range1 << range2
("strictly_right",)  # range1 >> range2
("not_left",)  # range1 &> range2
"not_right"  # range1 &< range2

# RESTRICTED (throws error)
"contains", "startswith", "endswith"
