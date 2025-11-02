# Extracted from: docs/core/types-and-schema.md
# Block number: 8
from fraiseql import type


# Department data is embedded in parent's JSONB
@type(sql_source="departments")
class Department:
    id: UUID
    name: str


# Employee view includes embedded department in JSONB
@type(sql_source="v_employees_with_dept")
class Employee:
    id: UUID
    name: str
    department: Department | None  # Uses embedded JSONB data
