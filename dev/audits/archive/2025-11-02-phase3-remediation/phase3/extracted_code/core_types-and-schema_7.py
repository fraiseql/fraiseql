# Extracted from: docs/core/types-and-schema.md
# Block number: 7
from fraiseql import type


# Department will be resolved via separate query
@type(sql_source="departments", resolve_nested=True)
class Department:
    id: UUID
    name: str


# Employee with department as a relation
@type(sql_source="employees")
class Employee:
    id: UUID
    name: str
    department_id: UUID  # Foreign key
    department: Department | None  # Will query departments table
