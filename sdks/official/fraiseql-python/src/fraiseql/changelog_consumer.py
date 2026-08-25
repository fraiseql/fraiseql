"""Pull-based changelog consumer for event-driven FraiseQL applications.

Provides :class:`ChangelogConsumer` — a high-level event loop that polls the
FraiseQL server's changelog REST endpoint, dispatches events to registered
handlers, and persists cursor state for durable at-least-once delivery.

At-least-once holds on both axes. A crash between dispatch and the checkpoint
save replays the batch, and a handler that *raises* leaves the cursor where it
was, so the event is redelivered rather than recorded as processed — bounded by
``max_redelivery_attempts``, after which it is dead-lettered. Set
``on_handler_error="skip"`` to advance past failures instead.

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
import time
from dataclasses import dataclass
from typing import Any, Protocol, runtime_checkable

import httpx

logger = logging.getLogger("fraiseql.changelog")

_HTTP_NOT_FOUND = 404

# Bounds on the ``from_now`` forward walk (#1058). Generous enough that an
# ordinary catch-up finishes well inside them, tight enough that a changelog
# under sustained write load cannot hold startup open indefinitely.
_TAIL_MAX_PAGES = 1000
_TAIL_DEADLINE_SECS = 30.0


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

# PollErrorHandler = an async callable (exception, consecutive_failures) -> None
PollErrorHandler = Any  # Callable[[Exception, int], Awaitable[None]]

# DeadLetterHandler = an async callable (event, exception) -> None
DeadLetterHandler = Any  # Callable[[ChangelogEvent, Exception], Awaitable[None]]

# on_handler_error policies
_ON_ERROR_HALT = "halt"
_ON_ERROR_SKIP = "skip"

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
        on_poll_error: Optional async callback ``(exc, consecutive_failures)``
            invoked after every failed poll.  Without it a permanently broken
            consumer — a revoked token, say — is indistinguishable from an idle
            one on the programmatic surface, since :meth:`run` never returns and
            never raises (#1061).  See also
            :attr:`consecutive_poll_failures` and :attr:`last_poll_error`.
        on_handler_error: What a raised handler means for the checkpoint.
            ``"halt"`` (default) stops the batch and leaves the cursor where it
            was, so the event is redelivered — the behaviour
            ``docs/operations/observer-idempotency.md`` describes.  ``"skip"``
            logs and advances anyway, which is the pre-#1078 behaviour.
        max_redelivery_attempts: How many times a failing event is redelivered
            under ``"halt"`` before it is dead-lettered and the cursor moves on
            (default ``5``).  Without a bound, one poison event blocks the
            consumer permanently.
        on_dead_letter: Optional async callback ``(event, exc)`` invoked when an
            event exhausts its redelivery attempts.  Register one to avoid
            losing the event at that point.
        client: Injectable :class:`httpx.AsyncClient` for testing.

    Raises:
        ValueError: If *on_handler_error* is neither ``"halt"`` nor ``"skip"``.
    """

    def __init__(  # noqa: PLR0913 — constructor genuinely needs all connection/polling params
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
        on_poll_error: PollErrorHandler | None = None,
        on_handler_error: str = _ON_ERROR_HALT,
        max_redelivery_attempts: int = 5,
        on_dead_letter: DeadLetterHandler | None = None,
        client: httpx.AsyncClient | None = None,
    ) -> None:
        if on_handler_error not in (_ON_ERROR_HALT, _ON_ERROR_SKIP):
            msg = (
                f"on_handler_error must be {_ON_ERROR_HALT!r} or {_ON_ERROR_SKIP!r}, "
                f"got {on_handler_error!r}"
            )
            raise ValueError(msg)
        self._on_poll_error = on_poll_error
        self._on_handler_error = on_handler_error
        self._max_redelivery_attempts = max_redelivery_attempts
        self._on_dead_letter = on_dead_letter
        self._redelivery_attempts: dict[int, int] = {}
        self._consecutive_poll_failures = 0
        self._last_poll_error: Exception | None = None
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
            # Apply the configured Authorization to the injected client too,
            # rather than silently dropping it (L-sdk-injected-client).
            if authorization is not None:
                client.headers["Authorization"] = authorization
        else:
            self._client = httpx.AsyncClient(headers=headers, timeout=timeout)
            self._owns_client = True

        if checkpoint_store is not None:
            self._checkpoint_store: CheckpointStore = checkpoint_store
        else:
            self._checkpoint_store = HttpCheckpointStore(self._client, self._base_url)

    # ─── Health ──────────────────────────────────────────────────────────────

    @property
    def consecutive_poll_failures(self) -> int:
        """Failed polls since the last successful one (``0`` when healthy)."""
        return self._consecutive_poll_failures

    @property
    def last_poll_error(self) -> Exception | None:
        """The most recent poll failure, or ``None`` if the last poll worked."""
        return self._last_poll_error

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
            await self._initialise_cursor(stop_event)

            current_interval = self._poll_interval

            while not stop_event.is_set():
                entries = await self._poll_once()

                if entries is None:
                    # A failed poll. Backs off like an idle changelog — the
                    # right pacing for a transient fault — but the caller can
                    # now tell the two apart, and hear about it.
                    await self._report_poll_error()
                    current_interval = min(
                        current_interval * self._backoff_factor,
                        self._max_poll_interval,
                    )
                elif entries:
                    last_ok, halted = await self._process_batch(entries)

                    # Checkpoint at the last event that was actually processed,
                    # not the last one received. If the first event of the batch
                    # failed there is nothing to record, and the batch is
                    # redelivered whole.
                    if last_ok is not None:
                        self._cursor = last_ok
                        await self._checkpoint_store.save(self._listener_id, last_ok)

                    if halted:
                        # Back off while the downstream recovers. This also
                        # stretches the redelivery window: at the defaults, the
                        # attempt budget spans ~30s of outage rather than ~5s.
                        current_interval = min(
                            current_interval * self._backoff_factor,
                            self._max_poll_interval,
                        )
                    else:
                        current_interval = self._poll_interval
                else:
                    # Exponential backoff on empty results
                    current_interval = min(
                        current_interval * self._backoff_factor,
                        self._max_poll_interval,
                    )

                # Sleep with early exit on stop.
                #
                # ``asyncio.TimeoutError``, not the builtin: they are the same
                # class only from 3.11 on. On 3.10 — which this package claims
                # to support — ``wait_for`` raises the asyncio one, the builtin
                # does not catch it, and the consumer died out of ``run()`` at
                # the end of its first sleep, every time (#1057).
                with contextlib.suppress(asyncio.TimeoutError):
                    await asyncio.wait_for(stop_event.wait(), timeout=current_interval)
        finally:
            if self._owns_client:
                await self._client.aclose()

    # ─── Internal ────────────────────────────────────────────────────────────

    async def _process_batch(
        self,
        entries: list[ChangelogEvent],
    ) -> tuple[int | None, bool]:
        """Dispatch a batch, stopping at the first event a handler could not process.

        Returns:
            ``(last_processed_cursor, halted)``.  *last_processed_cursor* is the
            newest event safe to checkpoint — ``None`` when the very first event
            failed.  *halted* says the batch stopped early, so the remainder is
            still pending.
        """
        last_ok: int | None = None

        for event in entries:
            failure = await self._dispatch(event)

            if failure is None:
                self._redelivery_attempts.pop(event._cursor, None)
                last_ok = event._cursor
                continue

            if self._on_handler_error == _ON_ERROR_SKIP:
                # Explicit opt-out: the caller has said a failed handler should
                # not hold the cursor back.
                last_ok = event._cursor
                continue

            attempts = self._redelivery_attempts.get(event._cursor, 0) + 1
            self._redelivery_attempts[event._cursor] = attempts

            if attempts >= self._max_redelivery_attempts:
                # Bounded: without this, one poison event blocks the consumer
                # forever — head-of-line blocking dressed up as durability.
                await self._dead_letter(event, failure)
                self._redelivery_attempts.pop(event._cursor, None)
                last_ok = event._cursor
                continue

            return last_ok, True

        return last_ok, False

    async def _initialise_cursor(self, stop_event: asyncio.Event | None = None) -> None:
        """Set the initial cursor based on startup_mode and checkpoint.

        Args:
            stop_event: Optional shutdown signal, forwarded to the ``from_now``
                tail walk so a slow startup can still be cancelled (#1058).
        """
        if self._startup_mode == "from_now":
            self._cursor = await self._fetch_tail_cursor(stop_event)
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

    async def _fetch_tail_cursor(self, stop_event: asyncio.Event | None = None) -> int:
        """Return the cursor of the newest changelog entry (0 if empty).

        ``from_now`` must skip *all* pre-existing history. The original code
        fetched the first page (``after_cursor=0, limit=1``) and checkpointed at
        its ``next_cursor`` — the *oldest* entry's cursor — so the very next poll
        replayed almost the entire changelog with side effects (H28).

        This uses the ``?latest=true`` tail query (a server returns only the
        newest entry's cursor) as a fast path, then pages forward to the true
        tail. The paging is what makes it correct against an older server that
        ignores ``?latest`` and answers as an ordinary ``after_cursor`` query:
        on a current server the first forward page is already empty (one extra
        round-trip); on an older one it walks to the real tail. Nothing is ever
        dispatched here, so no pre-existing row is processed.

        The forward walk is bounded (#1058). Its only natural exit is a page
        that comes back no further ahead — an *idle instant* on the changelog.
        Against one being written faster than the round-trip, every page
        advances and the walk never converges, so ``run()`` never reaches its
        polling loop, dispatches nothing, and cannot be stopped. A page cap, a
        wall-clock deadline and a *stop_event* check bound it; whatever cursor
        it stopped at is still a valid ``from_now`` tail, because nothing
        before it is ever dispatched.

        Args:
            stop_event: Optional shutdown signal, honoured between pages.
        """
        cursor = 0
        # Fast path: the newest entry's cursor on servers that support ?latest.
        resp = await self._client.get(
            f"{self._base_url}/api/observers/changelog",
            params={"latest": "true", "after_cursor": 0, "limit": self._batch_size},
        )
        resp.raise_for_status()
        next_cursor = resp.json().get("next_cursor")
        if next_cursor is not None:
            cursor = int(next_cursor)

        # Page forward to the true tail (correctness on servers that ignore
        # ?latest). ``after_cursor`` returns strictly-greater cursors, so this
        # advances monotonically and terminates when a page comes back empty —
        # or when one of the three bounds below trips.
        deadline = time.monotonic() + _TAIL_DEADLINE_SECS
        for page in range(_TAIL_MAX_PAGES):
            if stop_event is not None and stop_event.is_set():
                logger.info("Tail paging stopped at cursor=%d on shutdown", cursor)
                return cursor
            if time.monotonic() >= deadline:
                logger.warning(
                    "Tail paging hit its %.0fs deadline after %d pages at cursor=%d; "
                    "starting from here (the changelog is being written faster than "
                    "it can be walked)",
                    _TAIL_DEADLINE_SECS,
                    page,
                    cursor,
                )
                return cursor

            resp = await self._client.get(
                f"{self._base_url}/api/observers/changelog",
                params={"after_cursor": cursor, "limit": self._batch_size},
            )
            resp.raise_for_status()
            next_cursor = resp.json().get("next_cursor")
            if next_cursor is None or int(next_cursor) <= cursor:
                return cursor
            cursor = int(next_cursor)

        logger.warning(
            "Tail paging stopped at the %d-page cap at cursor=%d; starting from here",
            _TAIL_MAX_PAGES,
            cursor,
        )
        return cursor

    async def _poll_once(self) -> list[ChangelogEvent] | None:
        """Fetch one batch of changelog entries from the server.

        Returns:
            The batch (possibly empty) on success, or ``None`` when the poll
            failed.  ``[]`` and ``None`` are deliberately different: an idle
            changelog and a permanently broken consumer used to be the same
            value, so a caller had no way to tell them apart (#1061).
        """
        try:
            resp = await self._client.get(
                f"{self._base_url}/api/observers/changelog",
                params={
                    "after_cursor": self._cursor,
                    "limit": self._batch_size,
                },
            )
            resp.raise_for_status()
            body: dict[str, Any] = resp.json()
            raw_entries: list[dict[str, Any]] = body.get("entries", [])
        except (httpx.HTTPError, ValueError, AttributeError, TypeError) as exc:
            # The parse belongs inside the guard. Outside it, a proxy's HTML
            # error page behind a 200 killed run() outright, while a permanent
            # 401 — which will never self-heal — was swallowed forever. The
            # asymmetry ran exactly backwards.
            logger.exception("Failed to poll changelog")
            self._record_poll_failure(exc)
            return None

        self._consecutive_poll_failures = 0
        self._last_poll_error = None
        return [ChangelogEvent.from_row(row) for row in raw_entries]

    def _record_poll_failure(self, exc: Exception) -> None:
        """Count a failed poll and remember it."""
        self._consecutive_poll_failures += 1
        self._last_poll_error = exc

    async def _report_poll_error(self) -> None:
        """Hand the most recent poll failure to *on_poll_error*, if registered."""
        if self._on_poll_error is None or self._last_poll_error is None:
            return
        try:
            await self._on_poll_error(
                self._last_poll_error,
                self._consecutive_poll_failures,
            )
        except Exception:
            logger.exception("on_poll_error callback failed")

    async def _dispatch(self, event: ChangelogEvent) -> Exception | None:
        """Dispatch an event to all matching handlers (per-handler isolation).

        Every matching handler runs even if an earlier one raised — isolation
        between handlers is deliberate.  What changed is that the failure is now
        *reported* rather than only logged: the caller decides what a raised
        handler means for the checkpoint (#1078).

        Returns:
            The first exception raised by any handler, or ``None`` if they all
            succeeded.
        """
        keys_to_try: list[_RegistryKey] = [
            (event.object_type, event.modification_type),
            (event.object_type, "*"),
            ("*", event.modification_type),
            ("*", "*"),
        ]

        first_failure: Exception | None = None
        for key in keys_to_try:
            for handler in self._handlers.get(key, []):
                try:
                    await handler(event)
                except Exception as exc:  # noqa: PERF203 — intentional per-handler isolation
                    logger.exception(
                        "Handler %s failed for event %s",
                        getattr(handler, "__name__", repr(handler)),
                        event.id,
                    )
                    if first_failure is None:
                        first_failure = exc
        return first_failure

    async def _dead_letter(self, event: ChangelogEvent, exc: Exception) -> None:
        """Hand a repeatedly-failing event to *on_dead_letter* and give up on it."""
        logger.error(
            "Event %s (cursor=%d) failed %d times; dead-lettering and advancing",
            event.id,
            event._cursor,
            self._max_redelivery_attempts,
        )
        if self._on_dead_letter is None:
            return
        try:
            await self._on_dead_letter(event, exc)
        except Exception:
            logger.exception("on_dead_letter callback failed")
