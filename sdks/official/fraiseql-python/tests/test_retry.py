"""Tests for RetryConfig."""

import pytest

from fraiseql.errors import FraiseQLError, NetworkError, TimeoutError
from fraiseql.retry import RetryConfig

# ─── Defaults ─────────────────────────────────────────────────────────────────


def test_default_max_attempts():
    cfg = RetryConfig()
    assert cfg.max_attempts == 1


def test_default_base_delay():
    cfg = RetryConfig()
    assert cfg.base_delay == 1.0


def test_default_max_delay():
    cfg = RetryConfig()
    assert cfg.max_delay == 30.0


def test_default_jitter_enabled():
    cfg = RetryConfig()
    assert cfg.jitter is True


def test_default_retry_on_includes_network_and_timeout():
    cfg = RetryConfig()
    assert NetworkError in cfg.retry_on
    assert TimeoutError in cfg.retry_on


# ─── should_retry ─────────────────────────────────────────────────────────────


def test_should_retry_network_error():
    cfg = RetryConfig(max_attempts=3)
    assert cfg.should_retry(NetworkError("conn reset")) is True


def test_should_retry_timeout_error():
    cfg = RetryConfig(max_attempts=3)
    assert cfg.should_retry(TimeoutError("timed out")) is True


def test_should_not_retry_generic_fraiseql_error():
    cfg = RetryConfig(max_attempts=3)
    assert cfg.should_retry(FraiseQLError("generic")) is False


def test_should_not_retry_value_error():
    cfg = RetryConfig(max_attempts=3)
    assert cfg.should_retry(ValueError("bad input")) is False


# ─── delay_for ────────────────────────────────────────────────────────────────


def test_delay_increases_exponentially_without_jitter():
    cfg = RetryConfig(base_delay=1.0, max_delay=60.0, jitter=False)
    assert cfg.delay_for(0) == pytest.approx(1.0)
    assert cfg.delay_for(1) == pytest.approx(2.0)
    assert cfg.delay_for(2) == pytest.approx(4.0)
    assert cfg.delay_for(3) == pytest.approx(8.0)


def test_delay_capped_at_max_delay():
    cfg = RetryConfig(base_delay=1.0, max_delay=5.0, jitter=False)
    assert cfg.delay_for(10) == pytest.approx(5.0)


def test_delay_with_jitter_is_within_bounds():
    cfg = RetryConfig(base_delay=1.0, max_delay=60.0, jitter=True)
    # With jitter, delay should be >= base and <= base * 1.1 for attempt 0
    for _ in range(50):
        delay = cfg.delay_for(0)
        assert 1.0 <= delay <= 1.0 * 1.1 + 1e-9


# ─── Custom retry_on ──────────────────────────────────────────────────────────


def test_custom_retry_on():
    cfg = RetryConfig(max_attempts=3, retry_on=(ValueError,))
    assert cfg.should_retry(ValueError("x")) is True
    assert cfg.should_retry(NetworkError("y")) is False
