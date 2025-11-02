# Extracted from: docs/core/database-api.md
# Block number: 30
@dataclass
class OrderByInstructions:
    instructions: list[OrderByInstruction]


@dataclass
class OrderByInstruction:
    field: str
    direction: OrderDirection


class OrderDirection(Enum):
    ASC = "asc"
    DESC = "desc"
