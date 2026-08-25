#!/usr/bin/env python3
"""Streaming FraiseQL schema definition.

Events, chat messages, presence and metrics — the read side of a real-time
application, plus the subscriptions that push each of them.

The views this names are created by sql/setup.sql (singular Trinity naming:
tb_event/v_event, tb_message/v_message, tb_user_activity/v_user_activity,
tb_metric/v_metric, each exposing a JSONB `data` column with snake_case keys,
which FraiseQL projects to camelCase on the GraphQL surface).

Run: python3 schema.py
Output: schema.json (consumed by `fraiseql-cli compile`)

Requires the SDK: `pip install fraiseql` (or, inside this repository,
`pip install -e sdks/official/fraiseql-python`).
"""

from pathlib import Path

import fraiseql
from fraiseql import ID, DateTime, Decimal, Json


@fraiseql.type(sql_source="v_event")
class Event:
    """Something that happened, as recorded on the event stream."""

    id: ID
    type: str
    timestamp: DateTime
    data: Json
    created_at: DateTime


@fraiseql.type(sql_source="v_message")
class Message:
    """A chat message, denormalized with its author's name."""

    id: ID
    user_id: ID
    username: str
    content: str
    timestamp: DateTime
    reactions: int
    created_at: DateTime


@fraiseql.type(sql_source="v_user_activity")
class UserActivity:
    """A user's presence: where they are and when they were last seen."""

    id: ID
    username: str
    status: str
    last_seen: DateTime
    active_now: bool
    updated_at: DateTime


@fraiseql.type(sql_source="v_metric")
class LiveMetrics:
    """One sample of a named system metric."""

    id: ID
    metric: str
    value: Decimal
    timestamp: DateTime
    source: str
    created_at: DateTime


# `limit` is not declared on the list queries below: it is an auto-param the
# compiler adds to every list query, so declaring it would only shadow the one
# that paginates with one that filters.
@fraiseql.query(sql_source="v_event")
def events(type: str | None = None) -> list[Event]:
    """Get recent events, optionally of one type."""
    ...


@fraiseql.query(sql_source="v_event")
def event(id: ID) -> Event | None:
    """Get a specific event by ID."""
    ...


@fraiseql.query(sql_source="v_message")
def messages() -> list[Message]:
    """Get recent messages."""
    ...


@fraiseql.query(sql_source="v_user_activity")
def user_activity(id: ID) -> UserActivity | None:
    """Get one user's presence."""
    ...


@fraiseql.query(sql_source="v_metric")
def metrics(metric: str | None = None) -> list[LiveMetrics]:
    """Get live system metrics, optionally for one metric name."""
    ...


@fraiseql.subscription(topic="event_events")
def on_event() -> Event:
    """Subscribe to new events as they are recorded."""
    ...


@fraiseql.subscription(topic="message_events")
def on_message() -> Message:
    """Subscribe to new messages as they arrive."""
    ...


@fraiseql.subscription(topic="user_activity_events")
def on_user_status_change() -> UserActivity:
    """Subscribe to presence changes."""
    ...


@fraiseql.subscription(topic="metric_events")
def on_metric_update() -> LiveMetrics:
    """Subscribe to metric samples as they land."""
    ...


if __name__ == "__main__":
    output_path = Path(__file__).parent / "schema.json"
    fraiseql.export_schema(str(output_path))
    print(f"Schema exported to: {output_path}")
