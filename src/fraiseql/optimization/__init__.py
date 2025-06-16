"""Query optimization utilities for FraiseQL."""

from .dataloader import DataLoader, dataloader_context
from .registry import LoaderRegistry, get_loader
from .loaders import UserLoader, ProjectLoader, TasksByProjectLoader, GenericForeignKeyLoader

__all__ = [
    "DataLoader",
    "dataloader_context",
    "LoaderRegistry",
    "get_loader",
    "UserLoader",
    "ProjectLoader", 
    "TasksByProjectLoader",
    "GenericForeignKeyLoader",
]