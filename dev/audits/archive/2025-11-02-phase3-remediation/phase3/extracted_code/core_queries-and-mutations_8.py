# Extracted from: docs/core/queries-and-mutations.md
# Block number: 8
from fraiseql import field, type


@type
class User:
    first_name: str
    last_name: str

    @field(description="User's full display name")
    def display_name(self) -> str:
        return f"{self.first_name} {self.last_name}"
