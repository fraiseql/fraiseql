"""Query optimization utilities for FraiseQL."""

from .dataloader import DataLoader
from .registry import LoaderRegistry, get_loader
from .loaders import UserLoader, ProjectLoader, TasksByProjectLoader, GenericForeignKeyLoader

__all__ = [
    "DataLoader",
    "LoaderRegistry",
    "get_loader",
    "UserLoader",
    "ProjectLoader", 
    "TasksByProjectLoader",
    "GenericForeignKeyLoader",
]