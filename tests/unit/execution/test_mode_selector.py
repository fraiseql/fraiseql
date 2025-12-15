"""Tests for execution mode_selector module."""

from typing import Any
from unittest.mock import MagicMock

import pytest

from fraiseql.execution.mode_selector import ExecutionMode, ModeSelector


# Mock classes
class MockConfig:
    """Mock FraiseQL configuration."""

    def __init__(self, **kwargs: Any) -> None:
        # Defaults
        self.enable_mode_hints = True
        self.mode_hint_pattern = r"#\s*@mode:\s*(\w+)"
        self.execution_mode_priority = ["turbo", "passthrough", "normal"]
        self.entity_routing = False
        self.passthrough_complexity_limit = 50
        self.passthrough_max_depth = 3

        # Override with provided values
        for k, v in kwargs.items():
            setattr(self, k, v)


class MockTurboRegistry:
    """Mock TurboRegistry."""

    def __init__(self, registered_queries: dict[str, Any] | None = None) -> None:
        self._queries = registered_queries or {}

    def get(self, query: str) -> Any | None:
        return self._queries.get(query)

    def __len__(self) -> int:
        return len(self._queries)


class MockQueryAnalyzer:
    """Mock QueryAnalyzer."""

    def __init__(
        self,
        eligible: bool = True,
        complexity_score: int = 10,
        max_depth: int = 2,
    ) -> None:
        self._eligible = eligible
        self._complexity_score = complexity_score
        self._max_depth = max_depth

    def analyze_for_passthrough(self, query: str, variables: dict[str, Any]) -> MagicMock:
        result = MagicMock()
        result.eligible = self._eligible
        result.complexity_score = self._complexity_score
        result.max_depth = self._max_depth
        return result


class MockQueryRouter:
    """Mock QueryRouter."""

    def __init__(self, execution_mode: ExecutionMode | None = None) -> None:
        self._mode = execution_mode

    def determine_execution_mode(self, query: str) -> ExecutionMode | None:
        return self._mode


# Tests for ExecutionMode enum
@pytest.mark.unit
class TestExecutionMode:
    """Tests for ExecutionMode enum."""

    def test_execution_mode_values(self) -> None:
        """ExecutionMode has correct values."""
        assert ExecutionMode.TURBO.value == "turbo"
        assert ExecutionMode.PASSTHROUGH.value == "passthrough"
        assert ExecutionMode.NORMAL.value == "normal"

    def test_execution_mode_from_string(self) -> None:
        """Can create ExecutionMode from string value."""
        assert ExecutionMode("turbo") == ExecutionMode.TURBO
        assert ExecutionMode("passthrough") == ExecutionMode.PASSTHROUGH
        assert ExecutionMode("normal") == ExecutionMode.NORMAL

    def test_execution_mode_invalid_raises(self) -> None:
        """Invalid value raises ValueError."""
        with pytest.raises(ValueError):
            ExecutionMode("invalid")


# Tests for ModeSelector initialization
@pytest.mark.unit
class TestModeSelectorInit:
    """Tests for ModeSelector initialization."""

    def test_mode_selector_init(self) -> None:
        """ModeSelector initializes with config."""
        config = MockConfig()
        selector = ModeSelector(config)

        assert selector.config is config
        assert selector.turbo_registry is None
        assert selector.query_analyzer is None
        assert selector.query_router is None

    def test_set_turbo_registry(self) -> None:
        """set_turbo_registry stores registry."""
        config = MockConfig()
        selector = ModeSelector(config)
        registry = MockTurboRegistry()

        selector.set_turbo_registry(registry)

        assert selector.turbo_registry is registry

    def test_set_query_analyzer(self) -> None:
        """set_query_analyzer stores analyzer."""
        config = MockConfig()
        selector = ModeSelector(config)
        analyzer = MockQueryAnalyzer()

        selector.set_query_analyzer(analyzer)

        assert selector.query_analyzer is analyzer

    def test_set_query_router(self) -> None:
        """set_query_router stores router."""
        config = MockConfig()
        selector = ModeSelector(config)
        router = MockQueryRouter()

        selector.set_query_router(router)

        assert selector.query_router is router


# Tests for select_mode with mode hints
@pytest.mark.unit
class TestSelectModeWithHints:
    """Tests for select_mode with mode hints."""

    def test_select_mode_with_mode_hint_turbo(self) -> None:
        """Mode hint for turbo is respected."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "# @mode: turbo\nquery { users { id } }"
        result = selector.select_mode(query, {}, {})

        assert result == ExecutionMode.TURBO

    def test_select_mode_with_mode_hint_passthrough(self) -> None:
        """Mode hint for passthrough is respected."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "# @mode: passthrough\nquery { users { id } }"
        result = selector.select_mode(query, {}, {})

        assert result == ExecutionMode.PASSTHROUGH

    def test_select_mode_with_mode_hint_normal(self) -> None:
        """Mode hint for normal is respected."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "# @mode: normal\nquery { users { id } }"
        result = selector.select_mode(query, {}, {})

        assert result == ExecutionMode.NORMAL

    def test_select_mode_invalid_hint_ignored(self) -> None:
        """Invalid mode hint is ignored."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "# @mode: invalid\nquery { users { id } }"
        result = selector.select_mode(query, {}, {})

        # Should fall back to normal since no registry/analyzer
        assert result == ExecutionMode.NORMAL

    def test_select_mode_hints_disabled(self) -> None:
        """Mode hints are not processed when disabled."""
        config = MockConfig(enable_mode_hints=False)
        selector = ModeSelector(config)

        query = "# @mode: turbo\nquery { users { id } }"
        result = selector.select_mode(query, {}, {})

        # Should fall back to normal since hints disabled
        assert result == ExecutionMode.NORMAL


# Tests for select_mode with entity routing
@pytest.mark.unit
class TestSelectModeWithEntityRouting:
    """Tests for select_mode with entity routing."""

    def test_select_mode_entity_routing_enabled(self) -> None:
        """Entity routing returns mode when configured."""
        config = MockConfig(entity_routing=True)
        selector = ModeSelector(config)
        router = MockQueryRouter(execution_mode=ExecutionMode.TURBO)
        selector.set_query_router(router)

        query = "query { users { id } }"
        result = selector.select_mode(query, {}, {})

        assert result == ExecutionMode.TURBO

    def test_select_mode_entity_routing_returns_none(self) -> None:
        """Falls through when entity routing returns None."""
        config = MockConfig(entity_routing=True)
        selector = ModeSelector(config)
        router = MockQueryRouter(execution_mode=None)
        selector.set_query_router(router)

        query = "query { users { id } }"
        result = selector.select_mode(query, {}, {})

        # Falls through to normal since no turbo/passthrough available
        assert result == ExecutionMode.NORMAL


# Tests for select_mode with priority
@pytest.mark.unit
class TestSelectModeWithPriority:
    """Tests for select_mode respecting priority."""

    def test_select_mode_priority_turbo_first(self) -> None:
        """Turbo mode is selected when available and first in priority."""
        config = MockConfig(execution_mode_priority=["turbo", "passthrough", "normal"])
        selector = ModeSelector(config)

        # Set up turbo registry with a matching query
        registry = MockTurboRegistry({"query { users { id } }": True})
        selector.set_turbo_registry(registry)

        query = "query { users { id } }"
        result = selector.select_mode(query, {}, {})

        assert result == ExecutionMode.TURBO

    def test_select_mode_priority_passthrough_first(self) -> None:
        """Passthrough mode is selected when first and eligible."""
        config = MockConfig(execution_mode_priority=["passthrough", "turbo", "normal"])
        selector = ModeSelector(config)

        # Set up query analyzer that returns eligible
        analyzer = MockQueryAnalyzer(eligible=True, complexity_score=10, max_depth=2)
        selector.set_query_analyzer(analyzer)

        query = "query { users { id } }"
        result = selector.select_mode(query, {}, {})

        assert result == ExecutionMode.PASSTHROUGH

    def test_select_mode_default_normal(self) -> None:
        """Normal mode is returned when no other mode available."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "query { users { id } }"
        result = selector.select_mode(query, {}, {})

        assert result == ExecutionMode.NORMAL


# Tests for _extract_mode_hint
@pytest.mark.unit
class TestExtractModeHint:
    """Tests for _extract_mode_hint method."""

    def test_extract_mode_hint_found(self) -> None:
        """Extract mode hint when present."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "# @mode: turbo\nquery { users { id } }"
        result = selector._extract_mode_hint(query)

        assert result == ExecutionMode.TURBO

    def test_extract_mode_hint_not_found(self) -> None:
        """Return None when no mode hint."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "query { users { id } }"
        result = selector._extract_mode_hint(query)

        assert result is None

    def test_extract_mode_hint_invalid_mode(self) -> None:
        """Return None for invalid mode hint."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "# @mode: invalid_mode\nquery { users { id } }"
        result = selector._extract_mode_hint(query)

        assert result is None

    def test_extract_mode_hint_case_insensitive(self) -> None:
        """Mode hint is case insensitive."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "# @mode: TURBO\nquery { users { id } }"
        result = selector._extract_mode_hint(query)

        assert result == ExecutionMode.TURBO

    def test_extract_mode_hint_with_spaces(self) -> None:
        """Mode hint works with extra spaces."""
        config = MockConfig()
        selector = ModeSelector(config)

        query = "#   @mode:   passthrough\nquery { users { id } }"
        result = selector._extract_mode_hint(query)

        assert result == ExecutionMode.PASSTHROUGH


# Tests for _can_use_turbo
@pytest.mark.unit
class TestCanUseTurbo:
    """Tests for _can_use_turbo method."""

    def test_can_use_turbo_no_registry(self) -> None:
        """Returns False when no registry set."""
        config = MockConfig()
        selector = ModeSelector(config)

        result = selector._can_use_turbo("query { users { id } }")

        assert result is False

    def test_can_use_turbo_query_registered(self) -> None:
        """Returns True when query is registered."""
        config = MockConfig()
        selector = ModeSelector(config)
        registry = MockTurboRegistry({"query { users { id } }": True})
        selector.set_turbo_registry(registry)

        result = selector._can_use_turbo("query { users { id } }")

        assert result is True

    def test_can_use_turbo_query_not_registered(self) -> None:
        """Returns False when query is not registered."""
        config = MockConfig()
        selector = ModeSelector(config)
        registry = MockTurboRegistry({"other query": True})
        selector.set_turbo_registry(registry)

        result = selector._can_use_turbo("query { users { id } }")

        assert result is False


# Tests for _can_use_passthrough
@pytest.mark.unit
class TestCanUsePassthrough:
    """Tests for _can_use_passthrough method."""

    def test_can_use_passthrough_no_analyzer(self) -> None:
        """Returns False when no analyzer set."""
        config = MockConfig()
        selector = ModeSelector(config)

        result = selector._can_use_passthrough("query { users { id } }", {})

        assert result is False

    def test_can_use_passthrough_eligible(self) -> None:
        """Returns True when query is eligible."""
        config = MockConfig()
        selector = ModeSelector(config)
        analyzer = MockQueryAnalyzer(eligible=True, complexity_score=10, max_depth=2)
        selector.set_query_analyzer(analyzer)

        result = selector._can_use_passthrough("query { users { id } }", {})

        assert result is True

    def test_can_use_passthrough_not_eligible(self) -> None:
        """Returns False when query is not eligible."""
        config = MockConfig()
        selector = ModeSelector(config)
        analyzer = MockQueryAnalyzer(eligible=False)
        selector.set_query_analyzer(analyzer)

        result = selector._can_use_passthrough("query { users { id } }", {})

        assert result is False

    def test_can_use_passthrough_complexity_exceeded(self) -> None:
        """Returns False when complexity exceeds limit."""
        config = MockConfig(passthrough_complexity_limit=50)
        selector = ModeSelector(config)
        analyzer = MockQueryAnalyzer(eligible=True, complexity_score=100, max_depth=2)
        selector.set_query_analyzer(analyzer)

        result = selector._can_use_passthrough("query { users { id } }", {})

        assert result is False

    def test_can_use_passthrough_depth_exceeded(self) -> None:
        """Returns False when depth exceeds limit."""
        config = MockConfig(passthrough_max_depth=3)
        selector = ModeSelector(config)
        analyzer = MockQueryAnalyzer(eligible=True, complexity_score=10, max_depth=5)
        selector.set_query_analyzer(analyzer)

        result = selector._can_use_passthrough("query { users { id } }", {})

        assert result is False


# Tests for get_mode_metrics
@pytest.mark.unit
class TestGetModeMetrics:
    """Tests for get_mode_metrics method."""

    def test_get_mode_metrics(self) -> None:
        """get_mode_metrics returns expected metrics."""
        config = MockConfig()
        selector = ModeSelector(config)

        metrics = selector.get_mode_metrics()

        assert metrics["turbo_enabled"] is True
        assert metrics["passthrough_enabled"] is True
        assert metrics["mode_hints_enabled"] is True
        assert "priority" in metrics

    def test_get_mode_metrics_with_registry(self) -> None:
        """get_mode_metrics includes registry count when set."""
        config = MockConfig()
        selector = ModeSelector(config)
        registry = MockTurboRegistry({"q1": True, "q2": True, "q3": True})
        selector.set_turbo_registry(registry)

        metrics = selector.get_mode_metrics()

        assert metrics["turbo_queries_registered"] == 3

    def test_get_mode_metrics_uses_config_priority(self) -> None:
        """get_mode_metrics uses priority from config."""
        config = MockConfig(execution_mode_priority=["normal", "passthrough"])
        selector = ModeSelector(config)

        metrics = selector.get_mode_metrics()

        assert metrics["priority"] == ["normal", "passthrough"]


# Tests for custom mode hint pattern
@pytest.mark.unit
class TestCustomModeHintPattern:
    """Tests for custom mode hint patterns."""

    def test_custom_mode_hint_pattern(self) -> None:
        """Custom mode hint pattern is used."""
        config = MockConfig(mode_hint_pattern=r"--\s*mode=(\w+)")
        selector = ModeSelector(config)

        query = "-- mode=turbo\nquery { users { id } }"
        result = selector._extract_mode_hint(query)

        assert result == ExecutionMode.TURBO

    def test_default_pattern_not_matched_with_custom(self) -> None:
        """Default pattern doesn't match when custom is used."""
        config = MockConfig(mode_hint_pattern=r"--\s*mode=(\w+)")
        selector = ModeSelector(config)

        query = "# @mode: turbo\nquery { users { id } }"
        result = selector._extract_mode_hint(query)

        # Default pattern won't match
        assert result is None
