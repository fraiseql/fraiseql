"""Query optimization utilities for FraiseQL."""

from .dataloader import DataLoader, dataloader_context
from .loaders import (
    GenericForeignKeyLoader,
    ProjectLoader,
    TasksByProjectLoader,
    UserLoader,
)
from .n_plus_one_detector import (
    N1QueryDetectedError,
    N1QueryDetector,
    configure_detector,
    get_detector,
    n1_detection_context,
    track_resolver_execution,
)
from .registry import LoaderRegistry, get_loader

__all__ = [
    "DataLoader",
    "GenericForeignKeyLoader",
    "LoaderRegistry",
    "N1QueryDetectedError",
    "N1QueryDetector",
    "ProjectLoader",
    "TasksByProjectLoader",
    "UserLoader",
    "configure_detector",
    "dataloader_context",
    "get_detector",
    "get_loader",
    "n1_detection_context",
    "track_resolver_execution",
]
