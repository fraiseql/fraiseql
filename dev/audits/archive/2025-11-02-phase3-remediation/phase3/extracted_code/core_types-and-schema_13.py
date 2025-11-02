# Extracted from: docs/core/types-and-schema.md
# Block number: 13
from fraiseql import interface, type


@interface
class Node:
    id: UUID


@type(implements=[Node])
class User:
    id: UUID
    email: str
    name: str


@type(implements=[Node])
class Post:
    id: UUID
    title: str
    content: str
