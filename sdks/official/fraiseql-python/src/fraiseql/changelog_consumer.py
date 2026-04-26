"""Pull-based changelog consumer for event-driven FraiseQL applications.

Provides :class:`ChangelogConsumer` — a high-level event loop that polls the
FraiseQL server's changelog REST endpoint, dispatches events to registered
handlers, and persists cursor state for durable at-least-once delivery.

Example::

    import asyncio
    from fraiseql import ChangelogConsumer, ChangelogEvent

    consumer = ChangelogConsumer(
        base_url="http://localhost:8080",
        listener_id="my_app",
    )

    async def on_order_created(event: ChangelogEvent) -> None:
        print(f"New order: {event.object_id}")
        print(f"Data: {event.data}")

    consumer.on("Order", "INSERT", on_order_created)

    stop_event = asyncio.Event()
    await consumer.run(stop_event)
"""

from __future__ import annotations

import asyncio
import contextlib
import logging
from dataclasses import dataclass
from typing import Any, Protocol, runtime_checkable

import httpx

logger = logging.getLogger("fraiseql.changelog")

_HTTP_NOT_FOUND = 404


# ── ChangelogEvent ────────────────────────────────────────────────────────────


@dataclass(frozen=True, slots=True)
class ChangelogEvent:
    """A single entity change event, with Debezium envelope unwrapped.

    Attributes:
        id: Public UUID identity of the change log entry.
        object_type: Entity type (e.g. ``"Order"``).
        object_id: Entity instance ID (usually a UUID string).
        modification_type: One of ``INSERT``, ``UPDATE``, ``DELETE``, ``NOOP``.
        data: Entity state *after* the change (or *before* for ``DELETE``).
        before: Entity state *before* the change (for ``UPDATE`` / ``DELETE``).
        user_id: User who made the change (from ``fk_contact``), or ``None``.
        org_id: Organisation / tenant (from ``fk_customer_org``), or ``None``.
        status: Change status string, or ``None``.
        metadata: Extra metadata dict, or ``None``.
        created_at: ISO 8601 timestamp string, or ``None``.
        _cursor: Internal monotonic cursor (``pk_entity_change_log``). Not
            intended for handler code — used by the consumer for polling.
    """

    id: str
    object_type: str
    object_id: str
    modification_type: str
    data: dict[str, Any]
    before: dict[str, Any] | None
    user_id: str | None
    org_id: str | None
    status: str | None
    metadata: dict[str, Any] | None
    created_at: str | None
    _cursor: int

    @classmethod
    def from_row(cls, row: dict[str, Any]) -> ChangelogEvent:
        """Construct from a raw changelog REST response row.

        Unwraps the Debezium envelope in ``object_data``:

        - ``op = "c"`` (create / INSERT): ``data`` = ``after``
        - ``op = "u"`` (update): ``data`` = ``after``, ``before`` preserved
        - ``op = "d"`` (delete): ``data`` = ``before``, ``before`` preserved
        - ``op = "r"`` (read / snapshot): ``data`` = ``after``
        """
        object_data = row.get("object_data") or {}

        op = ""
        after: dict[str, Any] = {}
        before: dict[str, Any] | None = None

        if isinstance(object_data, dict) and "op" in object_data:
            # Debezium envelope present
            op = str(object_data.get("op", ""))
            raw_after = object_data.get("after")
            raw_before = object_data.get("before")
            after = raw_after if isinstance(raw_after, dict) else {}
            before = raw_before if isinstance(raw_before, dict) else None
        else:
            # Not a Debezium envelope — treat the whole value as data
            after = object_data if isinstance(object_data, dict) else {}

        # For DELETE, promote "before" to "data" (the entity is gone)
        data = before if op == "d" and before is not None else after

        return cls(
            id=str(row.get("id", "")),
            object_type=str(row.get("object_type", "")),
            object_id=str(row.get("object_id", "")),
            modification_type=str(row.get("modification_type", "")),
            data=data,
            before=before,
            user_id=row.get("user_id"),
            org_id=row.get("org_id"),
            status=row.get("status"),
            metadata=row.get("metadata"),
            created_at=row.get("created_at"),
            _cursor=int(row.get("cursor", 0)),
        )


# ── Checkpoint protocol ──────────────────────────────────────────────────────


@runtime_checkable
class CheckpointStore(Protocol):
    """Protocol for persisting the consumer's polling cursor.

    Implement this to provide custom checkpoint storage (e.g. a local file,
    Redis, or an external database). The default :class:`HttpCheckpointStore`
    delegates to the FraiseQL server's checkpoint REST endpoint.
    """

    async def load(self, listener_id: str) -> int | None:
        """Load the last saved cursor, or ``None`` if no checkpoint exists."""
        ...

    async def save(self, listener_id: str, last_cursor: int) -> None:
        """Persist the cursor value."""
        ...


class HttpCheckpointStore:
    """Checkpoint store backed by the FraiseQL server REST API.

    Uses ``GET /api/observers/checkpoint/:listener_id`` and
    ``PUT /api/observers/checkpoint/:listener_id``.
    """

    def __init__(self, client: httpx.AsyncClient, base_url: str) -> None:
        self._client = client
        self._base_url = base_url.rstrip("/")

    async def load(self, listener_id: str) -> int | None:
        """Load checkpoint from the server."""
        resp = await self._client.get(
            f"{self._base_url}/api/observers/checkpoint/{listener_id}",
        )
        if resp.status_code == _HTTP_NOT_FOUND:
            return None
        resp.raise_for_status()
        body: dict[str, Any] = resp.json()
        return int(body["last_cursor"])

    async def save(self, listener_id: str, last_cursor: int) -> None:
        """Save checkpoint to the server."""
        resp = await self._client.put(
            f"{self._base_url}/api/observers/checkpoint/{listener_id}",
            json={"last_cursor": last_cursor},
        )
        resp.raise_for_status()


# ── Handler type ──────────────────────────────────────────────────────────────

# Handler = an async callable accepting a ChangelogEvent
Handler = Any  # Callable[[ChangelogEvent], Awaitable[None]]

# Registry key: (object_type, modification_type)  — "*" means wildcard
_RegistryKey = tuple[str, str]


# ── ChangelogConsumer ─────────────────────────────────────────────────────────


class ChangelogConsumer:
    """Pull-based consumer that polls the FraiseQL changelog and dispatches events.

    Args:
        base_url: FraiseQL server base URL (e.g. ``"http://localhost:8080"``).
        listener_id: Unique identifier for this consumer instance (used for
            checkpoint persistence).
        poll_interval: Seconds between polls when events are found (default ``1.0``).
        max_poll_interval: Backoff ceiling in seconds (default ``60.0``).
        backoff_factor: Multiplier applied on empty polls (default ``2.0``).
        batch_size: Maximum entries to fetch per poll (default ``100``).
        startup_mode: ``"from_checkpoint"`` (default) resumes from the saved
            cursor. ``"from_now"`` skips historical events and starts from the
            current tail of the changelog.
        checkpoint_store: A :class:`CheckpointStore` implementation, or ``None``
            to use the built-in :class:`HttpCheckpointStore`.
        authorization: Optional ``Authorization`` header value.
        timeout: HTTP request timeout in seconds (default ``30.0``).
        client: Injectable :class:`httpx.AsyncClient` for testing.
    """

    def __init__(
        self,
        base_url: str,
        listener_id: str,
        *,
        poll_interval: float = 1.0,
        max_poll_interval: float = 60.0,
        backoff_factor: float = 2.0,
        batch_size: int = 100,
        startup_mode: str = "from_checkpoint",
        checkpoint_store: CheckpointStore | None = None,
        authorization: str | None = None,
        timeout: float = 30.0,
        client: httpx.AsyncClient | None = None,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._listener_id = listener_id
        self._poll_interval = poll_interval
        self._max_poll_interval = max_poll_interval
        self._backoff_factor = backoff_factor
        self._batch_size = batch_size
        self._startup_mode = startup_mode
        self._cursor: int = 0
        self._handlers: dict[_RegistryKey, list[Handler]] = {}

        headers: dict[str, str] = {}
        if authorization is not None:
            headers["Authorization"] = authorization

        if client is not None:
            self._client = client
            self._owns_client = False
        else:
            self._client = httpx.AsyncClient(headers=headers, timeout=timeout)
            self._owns_client = True

        if checkpoint_store is not None:
            self._checkpoint_store: CheckpointStore = checkpoint_store
        else:
            self._checkpoint_store = HttpCheckpointStore(self._client, self._base_url)

    # ─── Registration ────────────────────────────────────────────────────────

    def on(
        self,
        object_type: str,
        modification_type: str,
        handler: Handler,
    ) -> None:
        """Register an async handler for a specific event pattern.

        Args:
            object_type: Entity type to match (e.g. ``"Order"``), or ``"*"``
                for all types.
            modification_type: One of ``INSERT``, ``UPDATE``, ``DELETE``,
                ``NOOP``, or ``"*"`` for all.
            handler: An async callable ``(event: ChangelogEvent) -> None``.
        """
        key: _RegistryKey = (object_type, modification_type)
        self._handlers.setdefault(key, []).append(handler)

    # ─── Main loop ───────────────────────────────────────────────────────────

    async def run(self, stop_event: asyncio.Event) -> None:
        """Poll the changelog, dispatch events, and persist checkpoints.

        Runs until *stop_event* is set.

        Args:
            stop_event: An :class:`asyncio.Event` whose :meth:`~asyncio.Event.is_set`
                method signals shutdown.
        """
        try:
            await self._initialise_cursor()

            current_interval = self._poll_interval

            while not stop_event.is_set():
                entries = await self._poll_once()

                if entries:
                    for event in entries:
                        await self._dispatch(event)

                    # Persist checkpoint at the last cursor in the batch
                    last_cursor = entries[-1]._cursor
                    self._cursor = last_cursor
                    await self._checkpoint_store.save(self._listener_id, last_cursor)

                    # Reset backoff on successful fetch
                    current_interval = self._poll_interval
                else:
                    # Exponential backoff on empty results
                    current_interval = min(
                        current_interval * self._backoff_factor,
                        self._max_poll_interval,
                    )

                # Sleep with early exit on stop
                with contextlib.suppress(TimeoutError):
                    await asyncio.wait_for(stop_event.wait(), timeout=current_interval)
        finally:
            if self._owns_client:
                await self._client.aclose()

    # ─── Internal ────────────────────────────────────────────────────────────

    async def _initialise_cursor(self) -> None:
        """Set the initial cursor based on startup_mode and checkpoint."""
        if self._startup_mode == "from_now":
            # Fetch the current tail of the changelog
            resp = await self._client.get(
                f"{self._base_url}/api/observers/changelog",
                params={"after_cursor": 0, "limit": 1},
            )
            resp.raise_for_status()
            body: dict[str, Any] = resp.json()
            # If there are entries, use the latest cursor; otherwise start at 0
            if body.get("next_cursor") is not None:
                self._cursor = int(body["next_cursor"])
            # Persist so subsequent from_checkpoint starts here
            await self._checkpoint_store.save(self._listener_id, self._cursor)
        else:
            saved = await self._checkpoint_store.load(self._listener_id)
            if saved is not None:
                self._cursor = saved

        logger.info(
            "Consumer '%s' initialised with cursor=%d (mode=%s)",
            self._listener_id,
            self._cursor,
            self._startup_mode,
        )

    async def _poll_once(self) -> list[ChangelogEvent]:
        """Fetch one batch of changelog entries from the server."""
        try:
            resp = await self._client.get(
                f"{self._base_url}/api/observers/changelog",
                params={
                    "after_cursor": self._cursor,
                    "limit": self._batch_size,
                },
            )
            resp.raise_for_status()
        except httpx.HTTPError:
            logger.exception("Failed to poll changelog")
            return []

        body: dict[str, Any] = resp.json()
        raw_entries: list[dict[str, Any]] = body.get("entries", [])
        return [ChangelogEvent.from_row(row) for row in raw_entries]

    async def _dispatch(self, event: ChangelogEvent) -> None:
        """Dispatch an event to all matching handlers (per-handler isolation)."""
        keys_to_try: list[_RegistryKey] = [
            (event.object_type, event.modification_type),
            (event.object_type, "*"),
            ("*", event.modification_type),
            ("*", "*"),
        ]

        for key in keys_to_try:
            for handler in self._handlers.get(key, []):
                try:
                    await handler(event)
                except Exception:
                    logger.exception(
                        "Handler %s failed for event %s",
                        getattr(handler, "__name__", repr(handler)),
                        event.id,
                    )
