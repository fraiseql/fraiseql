"""Entity-aware query routing for optimal performance and cache consistency."""

from fraiseql.routing.config import EntityRoutingConfig
from fraiseql.routing.entity_extractor import EntityAnalysisResult, EntityExtractor
from fraiseql.routing.query_router import QueryRouter

__all__ = [
    "EntityAnalysisResult",
    "EntityExtractor",
    "EntityRoutingConfig",
    "QueryRouter",
]
