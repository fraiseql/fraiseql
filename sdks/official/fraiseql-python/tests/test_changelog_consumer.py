"""Tests for ChangelogConsumer, ChangelogEvent, and checkpoint stores."""

from __future__ import annotations

import asyncio
import json

import httpx
import pytest
import sniffio

from fraiseql.changelog_consumer import (
    _TAIL_MAX_PAGES,
    ChangelogConsumer,
    ChangelogEvent,
    HttpCheckpointStore,
)

# ── Helpers ──────────────────────────────────────────────────────────────────


def _mock_transport(handler):
    return httpx.MockTransport(handler)


def _json_response(body, status_code=200):
    return httpx.Response(status_code, json=body)


def _make_changelog_row(  # noqa: PLR0913 — test fixture factory with defaults
    *,
    cursor=1,
    obj_type="Order",
    obj_id="abc-123",
    mod_type="INSERT",
    op="c",
    after=None,
    before=None,
):
    """Build a raw changelog REST response row."""
    if after is None:
        after = {"id": obj_id, "status": "new"}
    object_data = {"op": op, "after": after}
    if before is not None:
        object_data["before"] = before
    return {
        "cursor": cursor,
        "id": "evt-001",
        "org_id": "acme",
        "user_id": "user-42",
        "object_type": obj_type,
        "object_id": obj_id,
        "modification_type": mod_type,
        "status": None,
        "object_data": object_data,
        "metadata": None,
        "created_at": "2026-01-01T00:00:00Z",
    }


# ── ChangelogEvent.from_row ─────────────────────────────────────────────────


class TestChangelogEventFromRow:
    def test_insert_debezium(self):
        row = _make_changelog_row(op="c", mod_type="INSERT")
        event = ChangelogEvent.from_row(row)

        assert event.object_type == "Order"
        assert event.object_id == "abc-123"
        assert event.modification_type == "INSERT"
        assert event.data == {"id": "abc-123", "status": "new"}
        assert event.before is None
        assert event._cursor == 1

    def test_update_debezium(self):
        row = _make_changelog_row(
            op="u",
            mod_type="UPDATE",
            after={"id": "abc-123", "status": "shipped"},
            before={"id": "abc-123", "status": "new"},
        )
        event = ChangelogEvent.from_row(row)

        assert event.data == {"id": "abc-123", "status": "shipped"}
        assert event.before == {"id": "abc-123", "status": "new"}

    def test_delete_debezium(self):
        row = _make_changelog_row(
            op="d",
            mod_type="DELETE",
            after=None,
            before={"id": "abc-123", "status": "new"},
        )
        # For DELETE, after is null in Debezium — set after to empty
        row["object_data"]["after"] = None
        event = ChangelogEvent.from_row(row)

        # DELETE promotes before to data
        assert event.data == {"id": "abc-123", "status": "new"}
        assert event.before == {"id": "abc-123", "status": "new"}

    def test_snapshot_debezium(self):
        row = _make_changelog_row(op="r", mod_type="INSERT")
        event = ChangelogEvent.from_row(row)

        assert event.data == {"id": "abc-123", "status": "new"}

    def test_non_debezium_envelope(self):
        """When object_data has no 'op' key, treat the whole dict as data."""
        row = _make_changelog_row()
        row["object_data"] = {"id": "abc-123", "name": "Test"}

        event = ChangelogEvent.from_row(row)
        assert event.data == {"id": "abc-123", "name": "Test"}
        assert event.before is None

    def test_missing_fields_default(self):
        event = ChangelogEvent.from_row({})
        assert event.id == ""
        assert event.object_type == ""
        assert event.data == {}
        assert event._cursor == 0

    def test_user_and_org_ids(self):
        row = _make_changelog_row()
        event = ChangelogEvent.from_row(row)
        assert event.user_id == "user-42"
        assert event.org_id == "acme"


# ── HttpCheckpointStore ─────────────────────────────────────────────────────


class TestHttpCheckpointStore:
    @pytest.mark.anyio
    async def test_load_returns_cursor(self):
        def handler(request):
            assert "/api/observers/checkpoint/my_app" in str(request.url)
            return _json_response({"last_cursor": 42, "updated_at": None})

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        store = HttpCheckpointStore(client, "http://test")
        result = await store.load("my_app")
        assert result == 42

    @pytest.mark.anyio
    async def test_load_returns_none_on_404(self):
        def handler(request):
            return httpx.Response(404)

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        store = HttpCheckpointStore(client, "http://test")
        result = await store.load("unknown")
        assert result is None

    @pytest.mark.anyio
    async def test_save_sends_put(self):
        captured = {}

        def handler(request):
            captured["method"] = request.method
            captured["body"] = json.loads(request.content)
            return _json_response({"message": "ok"})

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        store = HttpCheckpointStore(client, "http://test")
        await store.save("my_app", 99)

        assert captured["method"] == "PUT"
        assert captured["body"] == {"last_cursor": 99}


# ── Handler registration and dispatch ────────────────────────────────────────


class TestHandlerDispatch:
    @pytest.mark.anyio
    async def test_exact_match_handler(self):
        received = []

        async def on_insert(event):
            received.append(event)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            client=httpx.AsyncClient(),
        )
        consumer.on("Order", "INSERT", on_insert)

        event = ChangelogEvent.from_row(_make_changelog_row())
        await consumer._dispatch(event)

        assert len(received) == 1
        assert received[0].object_type == "Order"

    @pytest.mark.anyio
    async def test_wildcard_object_type(self):
        received = []

        async def on_any_insert(event):
            received.append(event)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            client=httpx.AsyncClient(),
        )
        consumer.on("*", "INSERT", on_any_insert)

        event = ChangelogEvent.from_row(_make_changelog_row())
        await consumer._dispatch(event)

        assert len(received) == 1

    @pytest.mark.anyio
    async def test_wildcard_modification_type(self):
        received = []

        async def on_any_order(event):
            received.append(event)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            client=httpx.AsyncClient(),
        )
        consumer.on("Order", "*", on_any_order)

        event = ChangelogEvent.from_row(_make_changelog_row())
        await consumer._dispatch(event)

        assert len(received) == 1

    @pytest.mark.anyio
    async def test_double_wildcard(self):
        received = []

        async def on_anything(event):
            received.append(event)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            client=httpx.AsyncClient(),
        )
        consumer.on("*", "*", on_anything)

        event = ChangelogEvent.from_row(_make_changelog_row())
        await consumer._dispatch(event)

        assert len(received) == 1

    @pytest.mark.anyio
    async def test_no_matching_handler(self):
        """Dispatch completes without error when no handlers match."""
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            client=httpx.AsyncClient(),
        )
        consumer.on("Product", "DELETE", lambda e: None)

        event = ChangelogEvent.from_row(_make_changelog_row())
        await consumer._dispatch(event)  # Should not raise

    @pytest.mark.anyio
    async def test_handler_error_isolated(self):
        """A failing handler does not prevent subsequent handlers from running."""
        second_called = []

        async def bad_handler(event):
            raise ValueError("boom")

        async def good_handler(event):
            second_called.append(event)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            client=httpx.AsyncClient(),
        )
        consumer.on("Order", "INSERT", bad_handler)
        consumer.on("*", "*", good_handler)

        event = ChangelogEvent.from_row(_make_changelog_row())
        await consumer._dispatch(event)

        assert len(second_called) == 1

    @pytest.mark.anyio
    async def test_multiple_handlers_same_key(self):
        calls = []

        async def h1(event):
            calls.append("h1")

        async def h2(event):
            calls.append("h2")

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            client=httpx.AsyncClient(),
        )
        consumer.on("Order", "INSERT", h1)
        consumer.on("Order", "INSERT", h2)

        event = ChangelogEvent.from_row(_make_changelog_row())
        await consumer._dispatch(event)

        assert calls == ["h1", "h2"]


# ── Polling loop ─────────────────────────────────────────────────────────────


def _skip_unless_asyncio():
    """Skip if the current async backend is not asyncio."""
    try:
        if sniffio.current_async_library() != "asyncio":
            pytest.skip("asyncio-only test")
    except sniffio.AsyncLibraryNotFoundError:
        pytest.skip("no async library detected")


class TestPollingLoop:
    """Tests that exercise ``consumer.run()`` — asyncio-only (uses ``asyncio.Event``)."""

    @pytest.mark.anyio
    async def test_poll_dispatches_and_checkpoints(self):
        """Full run loop: poll entries, dispatch, save checkpoint, then stop."""
        _skip_unless_asyncio()
        poll_count = 0
        checkpoint_saved = {}
        received_events = []

        def handler(request):
            nonlocal poll_count
            url = str(request.url)

            if "/changelog" in url:
                poll_count += 1
                if poll_count == 1:
                    return _json_response(
                        {
                            "entries": [
                                {
                                    "cursor": 10,
                                    "id": "e1",
                                    "org_id": None,
                                    "user_id": None,
                                    "object_type": "Order",
                                    "object_id": "o1",
                                    "modification_type": "INSERT",
                                    "status": None,
                                    "object_data": {"op": "c", "after": {"id": "o1"}},
                                    "metadata": None,
                                    "created_at": None,
                                },
                            ],
                            "next_cursor": 10,
                        }
                    )
                # Second poll returns empty → consumer backs off, then stop
                return _json_response({"entries": [], "next_cursor": None})

            if "/checkpoint" in url and request.method == "PUT":
                checkpoint_saved.update(json.loads(request.content))
                return _json_response({"message": "ok"})

            if "/checkpoint" in url and request.method == "GET":
                return httpx.Response(404)

            return _json_response({})

        async def on_order(event):
            received_events.append(event)

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test_app",
            poll_interval=0.01,
            max_poll_interval=0.02,
            client=client,
        )
        consumer.on("Order", "INSERT", on_order)

        stop = asyncio.Event()

        async def stop_after_polls():
            while poll_count < 2:
                await asyncio.sleep(0.01)
            stop.set()

        # gather(), not TaskGroup: TaskGroup is 3.11+, and this is one of only
        # two tests that enter run() at all, so using it made the whole polling
        # loop unreachable on the 3.10 the package claims to support (#1057).
        await asyncio.gather(consumer.run(stop), stop_after_polls())

        assert len(received_events) == 1
        assert received_events[0].object_id == "o1"
        assert checkpoint_saved["last_cursor"] == 10

    @pytest.mark.anyio
    async def test_poll_http_error_does_not_crash(self):
        """HTTP errors during polling are logged but don't crash the loop."""
        _skip_unless_asyncio()
        poll_count = 0

        def handler(request):
            nonlocal poll_count
            url = str(request.url)

            if "/changelog" in url:
                poll_count += 1
                if poll_count == 1:
                    return httpx.Response(500)
                return _json_response({"entries": [], "next_cursor": None})

            if "/checkpoint" in url and request.method == "GET":
                return httpx.Response(404)

            return _json_response({})

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
            client=client,
        )

        stop = asyncio.Event()

        async def stop_after():
            while poll_count < 2:
                await asyncio.sleep(0.01)
            stop.set()

        await asyncio.gather(consumer.run(stop), stop_after())

        assert poll_count >= 2  # Survived the 500 error

    @pytest.mark.anyio
    async def test_checkpoint_does_not_advance_past_a_failed_handler(self):
        """A raised handler must not be recorded as successful processing (#1078)."""
        _skip_unless_asyncio()
        saved = []
        stop = asyncio.Event()
        polls = 0

        def handler(request):
            nonlocal polls
            url = str(request.url)
            if "/checkpoint" in url and request.method == "GET":
                return httpx.Response(404)
            if "/checkpoint" in url:
                saved.append(json.loads(request.content)["last_cursor"])
                return _json_response({"message": "ok"})
            polls += 1
            if polls == 1:
                return _json_response(
                    {
                        "entries": [
                            _make_changelog_row(cursor=10),
                            _make_changelog_row(cursor=11),
                            _make_changelog_row(cursor=12),
                        ],
                        "next_cursor": 12,
                    }
                )
            return _json_response({"entries": [], "next_cursor": None})

        async def on_order(event):
            if event._cursor == 11:
                msg = "downstream database unreachable"
                raise RuntimeError(msg)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
            max_poll_interval=0.02,
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        consumer.on("Order", "INSERT", on_order)

        async def stop_after():
            while polls < 2:
                await asyncio.sleep(0.01)
            stop.set()

        await asyncio.gather(consumer.run(stop), stop_after())

        # 10 succeeded and is durably processed; 11 failed, so neither it nor 12
        # may be checkpointed away.
        assert saved == [10]
        assert consumer._cursor == 10

    @pytest.mark.anyio
    async def test_a_poison_event_is_dead_lettered_and_does_not_block_forever(self):
        """Halt-before-advance is bounded, or one bad row stops the consumer (#1078)."""
        _skip_unless_asyncio()
        dead_lettered = []
        stop = asyncio.Event()
        attempts = 0

        def handler(request):
            url = str(request.url)
            if "/checkpoint" in url and request.method == "GET":
                return httpx.Response(404)
            if "/checkpoint" in url:
                return _json_response({"message": "ok"})
            return _json_response({"entries": [_make_changelog_row(cursor=11)], "next_cursor": 11})

        async def always_fails(event):
            nonlocal attempts
            attempts += 1
            msg = "poison"
            raise RuntimeError(msg)

        async def on_dead_letter(event, exc):
            dead_lettered.append((event._cursor, str(exc)))
            stop.set()

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
            max_poll_interval=0.02,
            max_redelivery_attempts=3,
            on_dead_letter=on_dead_letter,
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        consumer.on("Order", "INSERT", always_fails)

        await consumer.run(stop)

        assert dead_lettered == [(11, "poison")]
        assert attempts == 3
        # Having dead-lettered it, the consumer moves past rather than stalling.
        assert consumer._cursor == 11

    @pytest.mark.anyio
    async def test_skip_policy_keeps_the_old_advance_behaviour(self):
        """`on_handler_error="skip"` is the documented opt-out (#1078)."""
        _skip_unless_asyncio()
        saved = []
        stop = asyncio.Event()
        polls = 0

        def handler(request):
            nonlocal polls
            url = str(request.url)
            if "/checkpoint" in url and request.method == "GET":
                return httpx.Response(404)
            if "/checkpoint" in url:
                saved.append(json.loads(request.content)["last_cursor"])
                return _json_response({"message": "ok"})
            polls += 1
            if polls == 1:
                return _json_response(
                    {
                        "entries": [
                            _make_changelog_row(cursor=10),
                            _make_changelog_row(cursor=11),
                        ],
                        "next_cursor": 11,
                    }
                )
            return _json_response({"entries": [], "next_cursor": None})

        async def on_order(event):
            if event._cursor == 11:
                msg = "boom"
                raise RuntimeError(msg)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
            max_poll_interval=0.02,
            on_handler_error="skip",
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        consumer.on("Order", "INSERT", on_order)

        async def stop_after():
            while polls < 2:
                await asyncio.sleep(0.01)
            stop.set()

        await asyncio.gather(consumer.run(stop), stop_after())
        assert saved == [11]

    @pytest.mark.anyio
    async def test_a_clean_batch_still_checkpoints_at_the_last_cursor(self):
        """The control: halting must not slow down the healthy path (#1078)."""
        _skip_unless_asyncio()
        saved = []
        stop = asyncio.Event()
        polls = 0
        seen = []

        def handler(request):
            nonlocal polls
            url = str(request.url)
            if "/checkpoint" in url and request.method == "GET":
                return httpx.Response(404)
            if "/checkpoint" in url:
                saved.append(json.loads(request.content)["last_cursor"])
                return _json_response({"message": "ok"})
            polls += 1
            if polls == 1:
                return _json_response(
                    {
                        "entries": [
                            _make_changelog_row(cursor=10),
                            _make_changelog_row(cursor=11),
                        ],
                        "next_cursor": 11,
                    }
                )
            return _json_response({"entries": [], "next_cursor": None})

        async def on_order(event):
            seen.append(event._cursor)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
            max_poll_interval=0.02,
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        consumer.on("Order", "INSERT", on_order)

        async def stop_after():
            while polls < 2:
                await asyncio.sleep(0.01)
            stop.set()

        await asyncio.gather(consumer.run(stop), stop_after())
        assert seen == [10, 11]
        assert saved == [11]

    @pytest.mark.anyio
    async def test_failed_poll_is_distinguishable_from_an_empty_changelog(self):
        """A failure must not look like "no new events" (#1061)."""

        def handler(request):
            return httpx.Response(401)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        assert await consumer._poll_once() is None

    @pytest.mark.anyio
    async def test_empty_changelog_is_not_a_failure(self):
        """The discriminator: an idle changelog must NOT count as failing."""

        def handler(request):
            return _json_response({"entries": [], "next_cursor": None})

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        assert await consumer._poll_once() == []
        assert consumer.consecutive_poll_failures == 0
        assert consumer.last_poll_error is None

    @pytest.mark.anyio
    async def test_persistent_failure_is_counted_and_reported(self):
        """A permanently broken consumer must be detectable programmatically."""
        _skip_unless_asyncio()
        reported = []
        stop = asyncio.Event()
        polls = 0

        def handler(request):
            nonlocal polls
            if "/checkpoint" in str(request.url):
                return httpx.Response(404)
            polls += 1
            return httpx.Response(401)

        async def on_poll_error(exc, consecutive):
            reported.append((type(exc).__name__, consecutive))

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
            max_poll_interval=0.02,
            on_poll_error=on_poll_error,
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )

        async def stop_after():
            while polls < 3:
                await asyncio.sleep(0.01)
            stop.set()

        await asyncio.gather(consumer.run(stop), stop_after())

        assert consumer.consecutive_poll_failures >= 3
        assert consumer.last_poll_error is not None
        assert [c for _, c in reported][:3] == [1, 2, 3]

    @pytest.mark.anyio
    async def test_failure_counter_resets_after_a_good_poll(self):
        _skip_unless_asyncio()
        stop = asyncio.Event()
        polls = 0

        def handler(request):
            nonlocal polls
            url = str(request.url)
            if "/checkpoint" in url and request.method == "GET":
                return httpx.Response(404)
            if "/checkpoint" in url:
                return _json_response({"message": "ok"})
            polls += 1
            if polls <= 2:
                return httpx.Response(500)
            return _json_response({"entries": [], "next_cursor": None})

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
            max_poll_interval=0.02,
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )

        async def stop_after():
            while polls < 3:
                await asyncio.sleep(0.01)
            stop.set()

        await asyncio.gather(consumer.run(stop), stop_after())
        assert consumer.consecutive_poll_failures == 0

    @pytest.mark.anyio
    async def test_a_malformed_200_body_does_not_kill_the_loop(self):
        """The inverse asymmetry: a proxy HTML page was fatal, a permanent 401 was not.

        `resp.json()` and `body.get("entries")` sat outside the guarded region,
        so a transient malformed response killed `run()` outright while an
        auth failure that will never self-heal was swallowed forever (#1061).
        """
        _skip_unless_asyncio()
        stop = asyncio.Event()
        polls = 0

        def handler(request):
            nonlocal polls
            if "/checkpoint" in str(request.url):
                return httpx.Response(404)
            polls += 1
            if polls == 1:
                return httpx.Response(200, text="<html>502 Bad Gateway</html>")
            return _json_response({"entries": [], "next_cursor": None})

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
            max_poll_interval=0.02,
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )

        async def stop_after():
            while polls < 2:
                await asyncio.sleep(0.01)
            stop.set()

        await asyncio.gather(consumer.run(stop), stop_after())
        assert polls >= 2  # survived the unparseable body

    @pytest.mark.anyio
    async def test_backoff_on_empty_results(self, monkeypatch):
        """Empty polls increase the interval via exponential backoff, to a cap.

        Measured from ``run()``. The previous version of this test recomputed
        ``min(interval * factor, max)`` in its own body and never entered the
        consumer at all, so deleting the whole back-off branch left it green
        (#1062).
        """
        intervals: list[float] = []
        stop = asyncio.Event()

        def handler(request):
            url = str(request.url)
            if "/checkpoint" in url and request.method == "GET":
                return _json_response({"listener_id": "test", "last_cursor": 0})
            return _json_response({"entries": [], "next_cursor": None})

        async def recording_wait_for(awaitable, timeout):
            # The sleep the consumer asks for IS the measurement; nothing here
            # recomputes it.
            intervals.append(timeout)
            awaitable.close()
            if len(intervals) >= 5:
                stop.set()
                return True
            # asyncio.TimeoutError is what the real wait_for raises on every
            # version; the builtin is a *different* class before 3.11 (#1057).
            raise asyncio.TimeoutError

        monkeypatch.setattr(asyncio, "wait_for", recording_wait_for)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=1.0,
            max_poll_interval=10.0,
            backoff_factor=2.0,
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        await consumer.run(stop)

        assert intervals == [2.0, 4.0, 8.0, 10.0, 10.0]

    @pytest.mark.anyio
    async def test_backoff_resets_after_a_non_empty_poll(self, monkeypatch):
        """A batch resets the interval; without it a busy consumer stays slow."""
        intervals: list[float] = []
        stop = asyncio.Event()
        polls = 0

        def handler(request):
            nonlocal polls
            url = str(request.url)
            if "/checkpoint" in url and request.method == "GET":
                return _json_response({"listener_id": "test", "last_cursor": 0})
            if "/checkpoint" in url:
                return _json_response({"ok": True})
            polls += 1
            # Empty, empty, then one batch: the third sleep must be back to 1.0.
            if polls == 3:
                return _json_response(
                    {"entries": [_make_changelog_row(cursor=7)], "next_cursor": 7}
                )
            return _json_response({"entries": [], "next_cursor": None})

        async def recording_wait_for(awaitable, timeout):
            intervals.append(timeout)
            awaitable.close()
            if len(intervals) >= 3:
                stop.set()
                return True
            raise asyncio.TimeoutError

        monkeypatch.setattr(asyncio, "wait_for", recording_wait_for)

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=1.0,
            max_poll_interval=10.0,
            backoff_factor=2.0,
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        await consumer.run(stop)

        assert intervals == [2.0, 4.0, 1.0]


# ── startup_mode = "from_now" ────────────────────────────────────────────────


class TestStartupMode:
    @pytest.mark.anyio
    async def test_from_checkpoint_loads_saved_cursor(self):
        def handler(request):
            url = str(request.url)
            if "/checkpoint" in url and request.method == "GET":
                return _json_response(
                    {
                        "listener_id": "test",
                        "last_cursor": 55,
                        "updated_at": None,
                    }
                )
            return _json_response({"entries": [], "next_cursor": None})

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            startup_mode="from_checkpoint",
            client=client,
        )
        await consumer._initialise_cursor()
        assert consumer._cursor == 55

    @pytest.mark.anyio
    async def test_from_checkpoint_no_saved_defaults_to_zero(self):
        def handler(request):
            url = str(request.url)
            if "/checkpoint" in url and request.method == "GET":
                return httpx.Response(404)
            return _json_response({"entries": [], "next_cursor": None})

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            startup_mode="from_checkpoint",
            client=client,
        )
        await consumer._initialise_cursor()
        assert consumer._cursor == 0

    @pytest.mark.anyio
    async def test_from_now_jumps_to_tail(self):
        checkpoint_saved = {}

        def handler(request):
            url = str(request.url)
            if "/changelog" in url:
                return _json_response({"entries": [], "next_cursor": 999})
            if "/checkpoint" in url and request.method == "PUT":
                checkpoint_saved.update(json.loads(request.content))
                return _json_response({"message": "ok"})
            return _json_response({})

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            startup_mode="from_now",
            client=client,
        )
        await consumer._initialise_cursor()
        assert consumer._cursor == 999
        assert checkpoint_saved["last_cursor"] == 999

    @pytest.mark.anyio
    async def test_from_now_empty_changelog(self):
        checkpoint_saved = {}

        def handler(request):
            url = str(request.url)
            if "/changelog" in url:
                return _json_response({"entries": [], "next_cursor": None})
            if "/checkpoint" in url and request.method == "PUT":
                checkpoint_saved.update(json.loads(request.content))
                return _json_response({"message": "ok"})
            return _json_response({})

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            startup_mode="from_now",
            client=client,
        )
        await consumer._initialise_cursor()
        assert consumer._cursor == 0
        assert checkpoint_saved["last_cursor"] == 0


# ── from_now starts at the real tail (H28) ───────────────────────────────────


def _seeded_changelog_handler(cursors, *, honor_latest, checkpoint_saved):
    """Mock a changelog server seeded with entries at the given ``cursors``.

    ``honor_latest=True`` models a Phase-09+ server: ``?latest=true`` returns the
    single newest entry and its cursor as ``next_cursor`` (the real tail).
    ``honor_latest=False`` models an older server that ignores ``latest`` and
    treats the request as an ordinary ``after_cursor`` paged query.
    """

    def _entry(cursor):
        return {
            "cursor": cursor,
            "id": f"evt-{cursor}",
            "org_id": None,
            "user_id": None,
            "object_type": "Order",
            "object_id": f"o{cursor}",
            "modification_type": "INSERT",
            "status": None,
            "object_data": {"op": "c", "after": {"id": f"o{cursor}"}},
            "metadata": None,
            "created_at": None,
        }

    def handler(request):
        url = request.url
        if url.path.endswith("/changelog"):
            params = url.params
            after = int(params.get("after_cursor") or 0)
            limit = int(params.get("limit") or 100)
            if honor_latest and params.get("latest") == "true":
                tail = cursors[-1] if cursors else None
                entries = [_entry(tail)] if tail is not None else []
                return _json_response({"entries": entries, "next_cursor": tail})
            page = [c for c in cursors if c > after][:limit]
            return _json_response(
                {"entries": [_entry(c) for c in page], "next_cursor": page[-1] if page else None}
            )
        if "/checkpoint" in str(url) and request.method == "PUT":
            checkpoint_saved.update(json.loads(request.content))
            return _json_response({"message": "ok"})
        if "/checkpoint" in str(url):
            return httpx.Response(404)
        return _json_response({})

    return handler


class TestFromNowTail:
    """``from_now`` must checkpoint at the newest cursor, never the oldest (H28)."""

    @pytest.mark.anyio
    async def test_uses_latest_tail_not_oldest(self):
        """New server: from_now jumps to the tail (50), not the first entry (10)."""
        checkpoint_saved = {}
        handler = _seeded_changelog_handler(
            [10, 20, 30, 40, 50], honor_latest=True, checkpoint_saved=checkpoint_saved
        )
        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="t",
            startup_mode="from_now",
            batch_size=2,
            client=client,
        )
        await consumer._initialise_cursor()
        assert consumer._cursor == 50
        assert checkpoint_saved["last_cursor"] == 50

    @pytest.mark.anyio
    async def test_pages_to_tail_on_old_server(self):
        """Older server ignoring ?latest: page forward to the true tail (50)."""
        checkpoint_saved = {}
        handler = _seeded_changelog_handler(
            [10, 20, 30, 40, 50], honor_latest=False, checkpoint_saved=checkpoint_saved
        )
        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="t",
            startup_mode="from_now",
            batch_size=2,
            client=client,
        )
        await consumer._initialise_cursor()
        assert consumer._cursor == 50
        assert checkpoint_saved["last_cursor"] == 50

    @pytest.mark.anyio
    async def test_tail_paging_is_bounded_on_a_changelog_still_being_written(self):
        """A busy changelog must not keep `from_now` startup paging forever (#1058).

        The loop's only exit was a page that came back no further ahead — an
        idle instant. Against a changelog receiving writes faster than the
        round-trip, every page advanced, so `_initialise_cursor` never returned:
        `run()` never reached its polling loop, dispatched nothing, saved no
        checkpoint, and ignored `stop_event`.
        """
        requests = 0
        # Above the real cap, so an unbounded loop trips this rather than
        # hanging the test runner. Derived from the constant, never guessed:
        # a hard-coded threshold below the cap would fail the fixed code.
        runaway = _TAIL_MAX_PAGES + 50

        def handler(request):
            nonlocal requests
            url = str(request.url)
            if "/checkpoint" in url:
                return _json_response({"message": "ok"})
            requests += 1
            if requests >= runaway:
                # Force termination so a broken loop fails an assertion below
                # instead of never ending.
                return _json_response({"entries": [], "next_cursor": None})
            # Every page advances: the shape of a continuously-written changelog.
            return _json_response({"entries": [], "next_cursor": requests * 10})

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="t",
            startup_mode="from_now",
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        cursor = await consumer._fetch_tail_cursor()

        assert requests < runaway, (
            f"tail paging made {requests} requests against a changelog that keeps "
            "advancing; it has no page cap or deadline"
        )
        # Whatever it stopped at is still a valid from_now tail: nothing before
        # it is ever dispatched.
        assert cursor > 0

    @pytest.mark.anyio
    async def test_tail_paging_stops_when_the_consumer_is_asked_to_stop(self):
        """A spinning startup must still be shut down cleanly (#1058)."""
        requests = 0
        stop = asyncio.Event()

        def handler(request):
            nonlocal requests
            url = str(request.url)
            if "/checkpoint" in url:
                return _json_response({"message": "ok"})
            requests += 1
            if requests == 3:
                stop.set()
            return _json_response({"entries": [], "next_cursor": requests * 10})

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="t",
            startup_mode="from_now",
            client=httpx.AsyncClient(transport=_mock_transport(handler)),
        )
        await consumer._fetch_tail_cursor(stop)

        # The fast path plus three forward pages; it must not keep going.
        assert requests <= 4

    @pytest.mark.anyio
    async def test_first_poll_replays_no_preexisting_rows(self):
        """After from_now init, the first poll returns zero pre-existing entries."""
        handler = _seeded_changelog_handler(
            [10, 20, 30, 40, 50], honor_latest=True, checkpoint_saved={}
        )
        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="t",
            startup_mode="from_now",
            batch_size=2,
            client=client,
        )
        await consumer._initialise_cursor()
        first_batch = await consumer._poll_once()
        assert first_batch == []


# ── Client lifecycle ─────────────────────────────────────────────────────────


class TestClientLifecycle:
    @pytest.mark.anyio
    async def test_owns_client_closes_on_run_exit(self):
        """When no client is injected, the consumer creates and closes its own."""
        _skip_unless_asyncio()

        def handler(request):
            url = str(request.url)
            if "/checkpoint" in url:
                return httpx.Response(404)
            return _json_response({"entries": [], "next_cursor": None})

        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
        )
        # Swap internal client and checkpoint store for testability
        mock_client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer._client = mock_client
        consumer._checkpoint_store = HttpCheckpointStore(mock_client, "http://test")
        consumer._owns_client = True

        stop = asyncio.Event()
        stop.set()  # Stop immediately

        await consumer.run(stop)
        assert consumer._client.is_closed

    @pytest.mark.anyio
    async def test_injected_client_receives_authorization_header(self):
        """An injected client still sends the configured Authorization (L-sdk-injected-client)."""
        captured = {}

        def handler(request):
            captured["auth"] = request.headers.get("Authorization")
            return _json_response({"entries": [], "next_cursor": None})

        injected = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="x",
            authorization="Bearer abc",
            client=injected,
        )
        await consumer._poll_once()
        await injected.aclose()
        assert captured["auth"] == "Bearer abc"

    @pytest.mark.anyio
    async def test_injected_client_not_closed(self):
        """When a client is injected, the consumer does not close it."""
        _skip_unless_asyncio()

        def handler(request):
            url = str(request.url)
            if "/checkpoint" in url:
                return httpx.Response(404)
            return _json_response({"entries": [], "next_cursor": None})

        client = httpx.AsyncClient(transport=_mock_transport(handler))
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=0.01,
            client=client,
        )

        stop = asyncio.Event()
        stop.set()

        await consumer.run(stop)
        assert not client.is_closed
        await client.aclose()
