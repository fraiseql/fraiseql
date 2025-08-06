"""Mode selection logic for query execution."""

import re
from enum import Enum
from typing import Any, Dict, Optional

from fraiseql.analysis.query_analyzer import QueryAnalyzer
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.turbo import TurboRegistry


class ExecutionMode(Enum):
    """Query execution modes."""

    TURBO = "turbo"  # TurboRouter execution
    PASSTHROUGH = "passthrough"  # Raw JSON passthrough
    NORMAL = "normal"  # Standard GraphQL execution


class ModeSelector:
    """Selects optimal execution mode for queries."""

    def __init__(self, config: FraiseQLConfig):
        """Initialize mode selector.

        Args:
            config: FraiseQL configuration
        """
        self.config = config
        self.turbo_registry: Optional[TurboRegistry] = None
        self.query_analyzer: Optional[QueryAnalyzer] = None
        self.mode_hint_pattern = re.compile(
            getattr(config, "mode_hint_pattern", r"#\s*@mode:\s*(\w+)")
        )

    def set_turbo_registry(self, registry: TurboRegistry):
        """Set TurboRouter registry.

        Args:
            registry: TurboRegistry instance
        """
        self.turbo_registry = registry

    def set_query_analyzer(self, analyzer: QueryAnalyzer):
        """Set query analyzer.

        Args:
            analyzer: QueryAnalyzer instance
        """
        self.query_analyzer = analyzer

    def select_mode(
        self, query: str, variables: Dict[str, Any], context: Dict[str, Any]
    ) -> ExecutionMode:
        """Select optimal execution mode for query.

        Args:
            query: GraphQL query string
            variables: Query variables
            context: Request context

        Returns:
            Selected execution mode
        """
        # Check for mode hint in query
        if getattr(self.config, "enable_mode_hints", True):
            mode_hint = self._extract_mode_hint(query)
            if mode_hint:
                return mode_hint

        # Check mode priority from config
        execution_mode_priority = getattr(
            self.config, "execution_mode_priority", ["turbo", "passthrough", "normal"]
        )

        for mode_name in execution_mode_priority:
            if mode_name == "turbo":
                if self._can_use_turbo(query):
                    return ExecutionMode.TURBO

            elif mode_name == "passthrough" and self._can_use_passthrough(query, variables):
                return ExecutionMode.PASSTHROUGH

        # Default to normal mode
        return ExecutionMode.NORMAL

    def _extract_mode_hint(self, query: str) -> Optional[ExecutionMode]:
        """Extract mode hint from query comment.

        Args:
            query: GraphQL query string

        Returns:
            Execution mode if hint found
        """
        match = self.mode_hint_pattern.search(query)
        if match:
            mode_str = match.group(1).lower()
            try:
                return ExecutionMode(mode_str)
            except ValueError:
                # Invalid mode hint
                pass

        return None

    def _can_use_turbo(self, query: str) -> bool:
        """Check if query can use TurboRouter.

        Args:
            query: GraphQL query string

        Returns:
            True if TurboRouter can handle the query
        """
        if not self.config.enable_turbo_router:
            return False

        if not self.turbo_registry:
            return False

        # Check if query is registered
        turbo_query = self.turbo_registry.get(query)
        return turbo_query is not None

    def _can_use_passthrough(self, query: str, variables: Dict[str, Any]) -> bool:
        """Check if query can use raw JSON passthrough.

        Args:
            query: GraphQL query string
            variables: Query variables

        Returns:
            True if passthrough can handle the query
        """
        if not self.config.json_passthrough_enabled:
            return False

        if not self.query_analyzer:
            return False

        # Analyze query
        analysis = self.query_analyzer.analyze_for_passthrough(query, variables)

        # Check against configuration limits
        passthrough_complexity_limit = getattr(self.config, "passthrough_complexity_limit", 50)
        if analysis.complexity_score > passthrough_complexity_limit:
            return False

        passthrough_max_depth = getattr(self.config, "passthrough_max_depth", 3)
        if analysis.max_depth > passthrough_max_depth:
            return False

        return analysis.eligible

    def get_mode_metrics(self) -> Dict[str, Any]:
        """Get metrics for mode selection.

        Returns:
            Dictionary of metrics
        """
        metrics = {
            "turbo_enabled": self.config.enable_turbo_router,
            "passthrough_enabled": self.config.json_passthrough_enabled,
            "mode_hints_enabled": getattr(self.config, "enable_mode_hints", True),
            "priority": getattr(
                self.config, "execution_mode_priority", ["turbo", "passthrough", "normal"]
            ),
        }

        if self.turbo_registry:
            metrics["turbo_queries_registered"] = len(self.turbo_registry)

        return metrics
