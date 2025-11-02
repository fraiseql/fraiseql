# Extracted from: docs/core/types-and-schema.md
# Block number: 14
from fraiseql import field, type


@interface
class Timestamped:
    created_at: datetime
    updated_at: datetime

    @field(description="Time since creation")
    def age(self) -> timedelta:
        return datetime.utcnow() - self.created_at


@type(implements=[Timestamped])
class Article:
    id: UUID
    title: str
    created_at: datetime
    updated_at: datetime

    @field(description="Time since creation")
    def age(self) -> timedelta:
        return datetime.utcnow() - self.created_at
