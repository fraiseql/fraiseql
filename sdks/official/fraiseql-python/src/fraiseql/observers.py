"""Observer authoring (#2147): run an action when a row of an entity changes.

An observer listens for an ``INSERT``, ``UPDATE`` or ``DELETE`` on an entity and runs
its actions: a webhook, a Slack message or an email. NO runtime behavior here; the
runtime is Rust. Each action is checked where the schema is compiled, against what the
server's observer runtime reads.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
from types import FunctionType
from typing import TYPE_CHECKING, Any, TypeVar

from fraiseql.registry import SchemaRegistry

if TYPE_CHECKING:
    from collections.abc import Callable

F = TypeVar("F", bound=FunctionType)

ObserverAction = dict[str, Any]
"""One action of an observer, as :func:`webhook`, :func:`slack` and :func:`email` build it."""

DEFAULT_SLACK_WEBHOOK_ENV = "SLACK_WEBHOOK_URL"
"""The variable a Slack action reads its incoming-webhook URL from, unless told another."""


@dataclass(frozen=True, kw_only=True)
class ObserverRetry:
    """How an observer retries an action that fails.

    Not :class:`fraiseql.RetryConfig`, which is the client's retry of a request.
    An observer given none runs on the server's own policy.
    """

    max_attempts: int
    backoff_strategy: str
    """``"fixed"``, ``"linear"`` or ``"exponential"``."""
    initial_delay_ms: int
    max_delay_ms: int


def webhook(  # noqa: PLR0913 — public API; all parameters are meaningful
    url: str | None = None,
    *,
    url_env: str | None = None,
    method: str | None = None,
    body_template: str | None = None,
    signing_secret_env: str | None = None,
    headers: dict[str, str] | None = None,
    headers_env: dict[str, str] | None = None,
) -> ObserverAction:
    """A webhook action: an HTTP request to ``url``, or to the URL in ``url_env``.

    Args:
        url: The URL, written into the schema.
        url_env: The environment variable the server reads the URL from at delivery.
            Exactly one of ``url`` and ``url_env`` is given.
        method: The HTTP method; absent, the server's default.
        body_template: The request body, a template with ``{{ field }}`` placeholders.
        signing_secret_env: The environment variable holding the key the server signs
            the request with.
        headers: Headers sent as written.
        headers_env: Headers whose values the server reads from these variables.

    Raises:
        ValueError: Neither or both of ``url`` and ``url_env`` are given.
    """
    if (url is None) == (url_env is None):
        msg = "fraiseql.webhook: give exactly one of url= and url_env= (the destination)."
        raise ValueError(msg)
    action: ObserverAction = {"type": "webhook"}
    optional = {
        "url": url,
        "url_env": url_env,
        "method": method,
        "body_template": body_template,
        "signing_secret_env": signing_secret_env,
        "headers": headers,
        "headers_env": headers_env,
    }
    action.update({key: value for key, value in optional.items() if value is not None})
    return action


def slack(
    channel: str,
    message: str,
    *,
    webhook_url: str | None = None,
    webhook_url_env: str | None = None,
) -> ObserverAction:
    """A Slack action: post ``message``, a ``{{ field }}`` template, to ``channel``.

    The incoming-webhook URL is read from :data:`DEFAULT_SLACK_WEBHOOK_ENV` unless
    ``webhook_url`` (written into the schema) or ``webhook_url_env`` names another.

    Raises:
        ValueError: Both ``webhook_url`` and ``webhook_url_env`` are given.
    """
    if webhook_url is not None and webhook_url_env is not None:
        msg = (
            "fraiseql.slack: give at most one of webhook_url= and webhook_url_env= "
            "(the destination)."
        )
        raise ValueError(msg)
    action: ObserverAction = {
        "type": "slack",
        "channel": channel,
        "message_template": message,
    }
    if webhook_url is not None:
        action["webhook_url"] = webhook_url
    else:
        action["webhook_url_env"] = webhook_url_env or DEFAULT_SLACK_WEBHOOK_ENV
    return action


def email(to: str, subject: str, body: str) -> ObserverAction:
    """An email action to ``to``; ``body`` is a ``{{ field }}`` template.

    The sender is the server's SMTP configuration: an action cannot set one.
    """
    return {"type": "email", "to": to, "subject": subject, "body_template": body}


def observer(
    *,
    entity: str,
    event: str,
    actions: list[ObserverAction],
    condition: str | None = None,
    retry: ObserverRetry | None = None,
) -> Callable[[F], F]:
    """Decorator to register an observer; the decorated function's name is its name.

    Args:
        entity: The type whose rows are observed.
        event: ``"INSERT"``, ``"UPDATE"`` or ``"DELETE"``.
        actions: What runs, built with :func:`webhook`, :func:`slack` and :func:`email`.
        condition: An expression on the row; the actions run only when it holds.
        retry: How a failed action is retried; absent, the server's own policy.

    Raises:
        TypeError: ``retry`` is not an :class:`ObserverRetry`.

    Examples:
        >>> @fraiseql.observer(
        ...     entity="Order",
        ...     event="INSERT",
        ...     actions=[fraiseql.email("ops@example.com", "New order", "Order {{ id }}")],
        ... )
        ... def order_created() -> None:
        ...     pass
    """
    if retry is not None and not isinstance(retry, ObserverRetry):
        msg = (
            f"fraiseql.observer: retry= must be a fraiseql.ObserverRetry "
            f"(got {retry.__class__.__name__!r})."
        )
        raise TypeError(msg)

    def decorator(f: F) -> F:
        SchemaRegistry.register_observer(
            name=f.__name__,
            entity=entity,
            event=event,
            actions=actions,
            condition=condition,
            retry=asdict(retry) if retry is not None else None,
        )
        return f

    return decorator
