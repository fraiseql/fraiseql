"""Query optimization utilities for FraiseQL."""

from .dataloader import DataLoader, dataloader_context
from .loaders import (
    GenericForeignKeyLoader,
    ProjectLoader,
    TasksByProjectLoader,
    UserLoader,
)
from .registry import LoaderRegistry, get_loader

__all__ = [
    "DataLoader",
    "GenericForeignKeyLoader",
    "LoaderRegistry",
    "ProjectLoader",
    "TasksByProjectLoader",
    "UserLoader",
    "dataloader_context",
    "get_loader",
]
