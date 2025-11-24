"""FraiseQL Organization Chart Schema
Demonstrates LTREE usage for employee hierarchies
"""

import fraiseql
from fraiseql.types.scalars import LTree


@fraiseql.fraise_type
class Employee:
    """Employee in the organization hierarchy."""

    id: int = fraiseql.field(primary_key=True)
    name: str
    title: str
    department: str
    salary: float
    org_path: LTree  # Hierarchical organizational path
    manager_id: int | None
    hire_date: str
    active: bool = True

    # Relationships
    @fraiseql.field
    def manager(self) -> "Employee | None":
        """Get the employee's manager."""
        return Employee.find_by(id=self.manager_id)

    @fraiseql.field
    def direct_reports(self) -> list["Employee"]:
        """Get employees who directly report to this employee."""
        return Employee.find_where(
            org_path__descendant_of=self.org_path,
            org_path__nlevel_eq=fraiseql.sql.Function("nlevel", self.org_path) + 1,
        )

    @fraiseql.field
    def all_reports(self) -> list["Employee"]:
        """Get all employees under this employee in the hierarchy."""
        return Employee.find_where(org_path__descendant_of=self.org_path).exclude(id=self.id)

    @fraiseql.field
    def peers(self) -> list["Employee"]:
        """Get employees at the same level with the same manager."""
        if not self.manager_id:
            return []

        parent_path = fraiseql.sql.Function("subpath", self.org_path, 0, -1)
        return Employee.find_where(
            org_path__descendant_of=parent_path,
            org_path__nlevel_eq=fraiseql.sql.Function("nlevel", self.org_path),
        ).exclude(id=self.id)


@fraiseql.fraise_query
class Query:
    """Root query for organization chart."""

    @fraiseql.field
    def employees(self, where: dict | None = None) -> list[Employee]:
        """Get all employees with optional filtering."""
        return Employee.find_where(**(where or {}))

    @fraiseql.field
    def employee(self, id: int) -> Employee | None:
        """Get a specific employee by ID."""
        return Employee.find_by(id=id)

    @fraiseql.field
    def department_employees(self, department: str) -> list[Employee]:
        """Get all employees in a department."""
        return Employee.find_where(department=department)

    @fraiseql.field
    def org_structure(self, root_path: str | None = None) -> list[Employee]:
        """Get organization structure starting from a root path."""
        root = root_path or "acme"
        return Employee.find_where(org_path__descendant_of=root).order_by("org_path")

    @fraiseql.field
    def managers(self) -> list[Employee]:
        """Get all employees who have direct reports."""
        # Employees who appear as managers in the org_path
        return Employee.find_where(
            id__in=fraiseql.sql.Subquery(
                Employee.select("manager_id").distinct().where(manager_id__isnull=False)
            )
        )


@fraiseql.fraise_mutation
class Mutation:
    """Mutations for organization management."""

    @fraiseql.field
    def update_employee_position(
        self, employee_id: int, new_manager_id: int | None, new_title: str | None = None
    ) -> Employee:
        """Update an employee's position in the organization."""
        employee = Employee.find_by(id=employee_id)
        if not employee:
            raise ValueError("Employee not found")

        # Update manager and title
        updates = {}
        if new_manager_id is not None:
            updates["manager_id"] = new_manager_id
        if new_title:
            updates["title"] = new_title

        # Update org_path based on new manager
        if new_manager_id:
            manager = Employee.find_by(id=new_manager_id)
            if manager:
                # New path: manager's path + employee name
                employee_name = employee.name.lower().replace(" ", "_")
                new_path = f"{manager.org_path}.{employee_name}"
                updates["org_path"] = new_path

        employee.update(**updates)
        return employee

    @fraiseql.field
    def add_employee(
        self,
        name: str,
        title: str,
        department: str,
        salary: float,
        manager_id: int | None,
        hire_date: str,
    ) -> Employee:
        """Add a new employee to the organization."""
        # Generate org_path
        if manager_id:
            manager = Employee.find_by(id=manager_id)
            if not manager:
                raise ValueError("Manager not found")
            employee_name = name.lower().replace(" ", "_")
            org_path = f"{manager.org_path}.{employee_name}"
        else:
            # Root level employee
            employee_name = name.lower().replace(" ", "_")
            org_path = f"acme.{department.lower()}.{employee_name}"

        return Employee.create(
            name=name,
            title=title,
            department=department,
            salary=salary,
            org_path=org_path,
            manager_id=manager_id,
            hire_date=hire_date,
        )


# Configure the GraphQL app
app = fraiseql.create_app(
    title="Organization Chart API",
    description="Hierarchical employee management with LTREE paths",
    version="1.0.0",
)
