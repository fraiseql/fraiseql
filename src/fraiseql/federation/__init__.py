"""Apollo Federation 2.0 support for FraiseQL.

Implements Federation Lite and Federation Standard modes with:
- Auto-key detection for 80% of users
- Simple @entity decorator (no configuration needed)
- Automatic entity resolution with _entities query
- Type extensions and computed fields

Progressive modes:
- Lite: Auto-keys only (80% of users)
- Standard: With extensions (15% of users)
- Advanced: All 18 directives (5% of users, Phase 17b)

Example:
    >>> from fraiseql import Schema, entity
    >>>
    >>> @entity  # Auto-detects 'id' as key
    ... class User:
    ...     id: str
    ...     name: str
    ...
    >>> schema = Schema(federation=True)
"""

from .config import FederationConfig, Presets
from .decorators import (
    clear_entity_registry,
    entity,
    extend_entity,
    external,
    get_entity_metadata,
    get_entity_registry,
)
from .directives import (
    DirectiveMetadata,
    get_directives,
    get_method_directives,
    provides,
    requires,
)
from .entities import EntitiesResolver
from .external_fields import (
    ExternalFieldInfo,
    ExternalFieldManager,
    extract_external_fields,
)

__all__ = [
    "DirectiveMetadata",
    "EntitiesResolver",
    "ExternalFieldInfo",
    "ExternalFieldManager",
    "FederationConfig",
    "Presets",
    "clear_entity_registry",
    "entity",
    "extend_entity",
    "external",
    "extract_external_fields",
    "get_directives",
    "get_entity_metadata",
    "get_entity_registry",
    "get_method_directives",
    "provides",
    "requires",
]
