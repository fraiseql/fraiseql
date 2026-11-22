"""Observer authoring (#2147): an observer per action kind, as the compiler reads it."""

import pytest

import fraiseql
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def _clean_registry() -> None:
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


def _observers() -> list[dict]:
    return SchemaRegistry.get_schema()["observers"]


def test_a_webhook_observer_carries_every_key_it_is_given() -> None:
    @fraiseql.observer(
        entity="Order",
        event="INSERT",
        actions=[
            fraiseql.webhook(
                url_env="HOOK_URL",
                method="PUT",
                body_template='{"total": {{ total }}}',
                signing_secret_env="HOOK_KEY",
                headers={"X-Source": "tests"},
                headers_env={"Authorization": "HOOK_TOKEN"},
            )
        ],
        retry=fraiseql.ObserverRetry(
            max_attempts=5, backoff_strategy="linear", initial_delay_ms=200, max_delay_ms=5000
        ),
    )
    def order_placed_webhook() -> None:
        pass

    assert _observers() == [
        {
            "name": "orderPlacedWebhook",
            "entity": "Order",
            "event": "INSERT",
            "actions": [
                {
                    "type": "webhook",
                    "url_env": "HOOK_URL",
                    "method": "PUT",
                    "body_template": '{"total": {{ total }}}',
                    "signing_secret_env": "HOOK_KEY",
                    "headers": {"X-Source": "tests"},
                    "headers_env": {"Authorization": "HOOK_TOKEN"},
                }
            ],
            "retry": {
                "max_attempts": 5,
                "backoff_strategy": "linear",
                "initial_delay_ms": 200,
                "max_delay_ms": 5000,
            },
        }
    ]


def test_a_webhook_given_only_its_url_authors_only_its_url() -> None:
    assert fraiseql.webhook("https://hooks.example/x") == {
        "type": "webhook",
        "url": "https://hooks.example/x",
    }


@pytest.mark.parametrize(
    "kwargs",
    [{}, {"url": "https://hooks.example/x", "url_env": "HOOK_URL"}],
    ids=["neither", "both"],
)
def test_a_webhook_has_exactly_one_destination(kwargs: dict) -> None:
    with pytest.raises(ValueError, match="exactly one of url= and url_env="):
        fraiseql.webhook(**kwargs)


def test_a_slack_action_reads_the_default_env_unless_told_otherwise() -> None:
    assert fraiseql.slack("#orders", "Order {{ id }} shipped") == {
        "type": "slack",
        "channel": "#orders",
        "message_template": "Order {{ id }} shipped",
        "webhook_url_env": "SLACK_WEBHOOK_URL",
    }
    assert fraiseql.slack("#o", "m", webhook_url_env="OPS_SLACK") == {
        "type": "slack",
        "channel": "#o",
        "message_template": "m",
        "webhook_url_env": "OPS_SLACK",
    }
    # A written URL is the one destination: the default env is not emitted beside it.
    assert fraiseql.slack("#o", "m", webhook_url="https://hooks.slack.example/x") == {
        "type": "slack",
        "channel": "#o",
        "message_template": "m",
        "webhook_url": "https://hooks.slack.example/x",
    }


def test_a_slack_action_has_one_destination() -> None:
    with pytest.raises(ValueError, match="one of webhook_url= and webhook_url_env="):
        fraiseql.slack("#o", "m", webhook_url="https://x", webhook_url_env="OPS_SLACK")


def test_an_email_action_names_its_body_template() -> None:
    assert fraiseql.email("ops@example.com", "Order deleted", "Order {{ id }}") == {
        "type": "email",
        "to": "ops@example.com",
        "subject": "Order deleted",
        "body_template": "Order {{ id }}",
    }


def test_a_condition_is_emitted_and_an_unset_retry_is_left_out() -> None:
    @fraiseql.observer(
        entity="Order",
        event="UPDATE",
        condition="status == 'shipped'",
        actions=[fraiseql.email("ops@example.com", "s", "b")],
    )
    def order_shipped() -> None:
        pass

    (definition,) = _observers()
    assert definition["condition"] == "status == 'shipped'"
    # No retry: the server's own policy applies, so no block is written.
    assert "retry" not in definition


def test_no_observer_leaves_the_section_out() -> None:
    assert "observers" not in SchemaRegistry.get_schema()


def test_retry_is_an_observer_retry_and_nothing_else() -> None:
    with pytest.raises(TypeError, match="ObserverRetry"):
        fraiseql.observer(
            entity="Order",
            event="INSERT",
            actions=[fraiseql.email("a@example.com", "s", "b")],
            retry={"max_attempts": 3},
        )


def test_an_observer_name_is_registered_once() -> None:
    def declare() -> None:
        @fraiseql.observer(
            entity="Order", event="INSERT", actions=[fraiseql.email("a@example.com", "s", "b")]
        )
        def order_created() -> None:
            pass

    declare()
    with pytest.raises(ValueError, match="'orderCreated' is already registered"):
        declare()
