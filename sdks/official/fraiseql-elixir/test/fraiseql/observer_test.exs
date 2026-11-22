defmodule FraiseQL.ObserverTest do
  @moduledoc """
  Observer authoring (#2147): an observer per action kind, as the compiler reads it.
  """
  use ExUnit.Case

  alias FraiseQL.{Observer, SchemaExporter}

  defmodule ObserverSchema do
    use FraiseQL.Schema

    fraiseql_type "Order", sql_source: "v_order" do
      field(:id, :id, nullable: false)
    end

    fraiseql_observer(:order_placed_webhook,
      entity: "Order",
      event: "INSERT",
      actions: [
        FraiseQL.Observer.webhook(
          url_env: "HOOK_URL",
          method: "PUT",
          body_template: ~s({"total": {{ total }}}),
          signing_secret_env: "HOOK_KEY",
          headers: %{"X-Source" => "tests"},
          headers_env: %{"Authorization" => "HOOK_TOKEN"}
        )
      ],
      retry: %FraiseQL.RetryConfig{
        max_attempts: 5,
        backoff_strategy: "linear",
        initial_delay_ms: 200,
        max_delay_ms: 5000
      }
    )

    fraiseql_observer(:order_shipped,
      entity: "Order",
      event: "UPDATE",
      condition: "status == 'shipped'",
      actions: [FraiseQL.Observer.email("ops@example.com", "s", "b")]
    )
  end

  defmodule NoObserverSchema do
    use FraiseQL.Schema

    fraiseql_type "Order", sql_source: "v_order" do
      field(:id, :id, nullable: false)
    end
  end

  defp observers(module) do
    module |> SchemaExporter.export() |> Jason.decode!() |> Map.fetch!("observers")
  end

  test "a webhook observer carries every key it is given" do
    assert hd(observers(ObserverSchema)) == %{
             "name" => "orderPlacedWebhook",
             "entity" => "Order",
             "event" => "INSERT",
             "actions" => [
               %{
                 "type" => "webhook",
                 "url_env" => "HOOK_URL",
                 "method" => "PUT",
                 "body_template" => ~s({"total": {{ total }}}),
                 "signing_secret_env" => "HOOK_KEY",
                 "headers" => %{"X-Source" => "tests"},
                 "headers_env" => %{"Authorization" => "HOOK_TOKEN"}
               }
             ],
             "retry" => %{
               "max_attempts" => 5,
               "backoff_strategy" => "linear",
               "initial_delay_ms" => 200,
               "max_delay_ms" => 5000
             }
           }
  end

  test "a condition is emitted and an unset retry is left out" do
    shipped = Enum.find(observers(ObserverSchema), &(&1["name"] == "orderShipped"))
    assert shipped["condition"] == "status == 'shipped'"
    # No retry: the server's own policy applies, so no block is written.
    refute Map.has_key?(shipped, "retry")
  end

  test "no observer leaves the section out" do
    decoded = NoObserverSchema |> SchemaExporter.export() |> Jason.decode!()
    refute Map.has_key?(decoded, "observers")
  end

  test "a webhook given only its url authors only its url" do
    assert Observer.webhook(url: "https://hooks.example/x") ==
             %{"type" => "webhook", "url" => "https://hooks.example/x"}
  end

  test "a webhook has exactly one destination" do
    for destination <- [[], [url: "https://hooks.example/x", url_env: "HOOK_URL"]] do
      assert_raise ArgumentError, ~r/exactly one of url: and url_env:/, fn ->
        Observer.webhook(destination)
      end
    end
  end

  test "a slack action reads the default env unless told otherwise" do
    assert Observer.slack("#orders", "Order {{ id }} shipped") == %{
             "type" => "slack",
             "channel" => "#orders",
             "message_template" => "Order {{ id }} shipped",
             "webhook_url_env" => "SLACK_WEBHOOK_URL"
           }

    assert Observer.slack("#o", "m", webhook_url_env: "OPS_SLACK")["webhook_url_env"] ==
             "OPS_SLACK"

    # A written URL is the one destination: the default env is not emitted beside it.
    assert Observer.slack("#o", "m", webhook_url: "https://hooks.slack.example/x") == %{
             "type" => "slack",
             "channel" => "#o",
             "message_template" => "m",
             "webhook_url" => "https://hooks.slack.example/x"
           }
  end

  test "a slack action has one destination" do
    assert_raise ArgumentError, ~r/at most one of webhook_url: and webhook_url_env:/, fn ->
      Observer.slack("#o", "m", webhook_url: "https://x", webhook_url_env: "OPS_SLACK")
    end
  end

  test "an email action names its body template" do
    assert Observer.email("ops@example.com", "Order deleted", "Order {{ id }}") == %{
             "type" => "email",
             "to" => "ops@example.com",
             "subject" => "Order deleted",
             "body_template" => "Order {{ id }}"
           }
  end

  test "retry is a FraiseQL.RetryConfig and nothing else" do
    assert_raise ArgumentError, ~r/retry: must be a %FraiseQL.RetryConfig\{\}/, fn ->
      Code.compile_string("""
      defmodule FraiseQL.ObserverTest.KeywordRetry do
        use FraiseQL.Schema
        fraiseql_observer :order_created,
          entity: "Order",
          event: "INSERT",
          actions: [FraiseQL.Observer.email("a@example.com", "s", "b")],
          retry: [max_attempts: 3]
      end
      """)
    end
  end

  test "an observer name is declared once" do
    assert_raise ArgumentError, ~r/duplicate observer name "orderCreated"/, fn ->
      Code.compile_string("""
      defmodule FraiseQL.ObserverTest.DuplicateObserver do
        use FraiseQL.Schema
        fraiseql_observer :order_created,
          entity: "Order",
          event: "INSERT",
          actions: [FraiseQL.Observer.email("a@example.com", "s", "b")]
        fraiseql_observer :order_created,
          entity: "Order",
          event: "INSERT",
          actions: [FraiseQL.Observer.email("a@example.com", "s", "b")]
      end
      """)
    end
  end
end
