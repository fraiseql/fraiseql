# Extracted from: docs/core/types-and-schema.md
# Block number: 15
from fraiseql import field, type


@interface
class Searchable:
    search_text: str


@interface
class Taggable:
    tags: list[str]


@type(implements=[Node, Searchable, Taggable])
class Document:
    id: UUID
    title: str
    content: str
    tags: list[str]

    @field
    def search_text(self) -> str:
        return f"{self.title} {self.content}"
