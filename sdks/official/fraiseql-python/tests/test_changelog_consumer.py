"""Tests for ChangelogConsumer, ChangelogEvent, and checkpoint stores."""

from __future__ import annotations

import asyncio
import json

import httpx
import pytest
import sniffio

from fraiseql.changelog_consumer import (
    ChangelogConsumer,
    ChangelogEvent,
    HttpCheckpointStore,
)

# ── Helpers ──────────────────────────────────────────────────────────────────


def _mock_transport(handler):
    return httpx.MockTransport(handler)


def _json_response(body, status_code=200):
    return httpx.Response(status_code, json=body)


def _make_changelog_row(
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

        async with asyncio.TaskGroup() as tg:
            tg.create_task(consumer.run(stop))
            tg.create_task(stop_after_polls())

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

        async with asyncio.TaskGroup() as tg:
            tg.create_task(consumer.run(stop))
            tg.create_task(stop_after())

        assert poll_count >= 2  # Survived the 500 error

    @pytest.mark.anyio
    async def test_backoff_on_empty_results(self):
        """Empty polls increase the interval via exponential backoff."""
        consumer = ChangelogConsumer(
            base_url="http://test",
            listener_id="test",
            poll_interval=1.0,
            max_poll_interval=10.0,
            backoff_factor=2.0,
            client=httpx.AsyncClient(),
        )
        # Simulate: no entries, check that the consumer would compute the right interval
        # We test this via the internal state after a couple of empty polls
        # Rather than timing real sleeps, we verify the backoff math directly.
        assert consumer._poll_interval == 1.0
        assert consumer._max_poll_interval == 10.0

        # backoff: 1.0 * 2.0 = 2.0, then 2.0 * 2.0 = 4.0, capped at 10.0
        interval = consumer._poll_interval
        for expected in [2.0, 4.0, 8.0, 10.0, 10.0]:
            interval = min(interval * consumer._backoff_factor, consumer._max_poll_interval)
            assert interval == expected


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
