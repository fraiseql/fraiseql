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

from .batch_executor import (
    BatchExecutor,
    ConcurrentBatchExecutor,
    PerRequestBatchExecutor,
)
from .computed_fields import (
    ComputedField,
    ComputedFieldValidator,
    extract_computed_fields,
    get_all_field_dependencies,
    validate_all_computed_fields,
)
from .config import FederationConfig, Presets
from .dataloader import DataLoaderStats, EntityDataLoader
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
from .sdl_generator import (
    SDLGenerator,
    generate_entity_sdl,
    generate_schema_sdl,
)
from .service_query import (
    ServiceQueryResolver,
    create_service_resolver,
    get_default_resolver,
    reset_default_resolver,
)

__all__ = [
    "BatchExecutor",
    "ComputedField",
    "ComputedFieldValidator",
    "ConcurrentBatchExecutor",
    "DataLoaderStats",
    "DirectiveMetadata",
    "EntitiesResolver",
    "EntityDataLoader",
    "ExternalFieldInfo",
    "ExternalFieldManager",
    "FederationConfig",
    "PerRequestBatchExecutor",
    "Presets",
    "SDLGenerator",
    "ServiceQueryResolver",
    "clear_entity_registry",
    "create_service_resolver",
    "entity",
    "extend_entity",
    "external",
    "extract_computed_fields",
    "extract_external_fields",
    "generate_entity_sdl",
    "generate_schema_sdl",
    "get_all_field_dependencies",
    "get_default_resolver",
    "get_directives",
    "get_entity_metadata",
    "get_entity_registry",
    "get_method_directives",
    "provides",
    "requires",
    "reset_default_resolver",
    "validate_all_computed_fields",
]
