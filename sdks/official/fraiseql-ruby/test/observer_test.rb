# frozen_string_literal: true

require "test_helper"

# Observer authoring (#2147): an observer per action kind, as the compiler reads it.
class ObserverTest < Minitest::Test
  def action_of(&block)
    schema = FraiseQL::Schema.new
    schema.observer(:probe, entity: "Order", event: "INSERT", &block)
    schema.to_h["observers"].first["actions"].first
  end

  def test_a_webhook_observer_carries_every_key_it_is_given
    schema = FraiseQL::Schema.new
    retry_policy = FraiseQL::ObserverRetry.new(
      max_attempts: 5, backoff_strategy: "linear", initial_delay_ms: 200, max_delay_ms: 5000
    )
    schema.observer :order_placed_webhook, entity: "Order", event: "INSERT", retry_: retry_policy do |o|
      o.webhook url_env: "HOOK_URL", method: "PUT", body_template: '{"total": {{ total }}}',
                signing_secret_env: "HOOK_KEY", headers: { "X-Source" => "tests" },
                headers_env: { "Authorization" => "HOOK_TOKEN" }
    end

    assert_equal(
      [{
        "name" => "orderPlacedWebhook", "entity" => "Order", "event" => "INSERT",
        "actions" => [{
          "type" => "webhook", "url_env" => "HOOK_URL", "method" => "PUT",
          "body_template" => '{"total": {{ total }}}', "signing_secret_env" => "HOOK_KEY",
          "headers" => { "X-Source" => "tests" }, "headers_env" => { "Authorization" => "HOOK_TOKEN" }
        }],
        "retry" => {
          "max_attempts" => 5, "backoff_strategy" => "linear",
          "initial_delay_ms" => 200, "max_delay_ms" => 5000
        }
      }],
      schema.to_h["observers"]
    )
  end

  def test_a_webhook_given_only_its_url_authors_only_its_url
    assert_equal({ "type" => "webhook", "url" => "https://hooks.example/x" },
                 action_of { |o| o.webhook url: "https://hooks.example/x" })
  end

  def test_a_webhook_has_exactly_one_destination
    [{}, { url: "https://hooks.example/x", url_env: "HOOK_URL" }].each do |destination|
      error = assert_raises(ArgumentError) { action_of { |o| o.webhook(**destination) } }
      assert_includes error.message, "exactly one of url: and url_env:"
    end
  end

  def test_a_slack_action_reads_the_default_env_unless_told_otherwise
    assert_equal({ "type" => "slack", "channel" => "#orders",
                   "message_template" => "Order {{ id }} shipped",
                   "webhook_url_env" => "SLACK_WEBHOOK_URL" },
                 action_of { |o| o.slack "#orders", "Order {{ id }} shipped" })
    assert_equal({ "type" => "slack", "channel" => "#o", "message_template" => "m",
                   "webhook_url_env" => "OPS_SLACK" },
                 action_of { |o| o.slack "#o", "m", webhook_url_env: "OPS_SLACK" })
    # A written URL is the one destination: the default env is not emitted beside it.
    assert_equal({ "type" => "slack", "channel" => "#o", "message_template" => "m",
                   "webhook_url" => "https://hooks.slack.example/x" },
                 action_of { |o| o.slack "#o", "m", webhook_url: "https://hooks.slack.example/x" })
  end

  def test_a_slack_action_has_one_destination
    error = assert_raises(ArgumentError) do
      action_of { |o| o.slack "#o", "m", webhook_url: "https://x", webhook_url_env: "OPS_SLACK" }
    end
    assert_includes error.message, "at most one of webhook_url: and webhook_url_env:"
  end

  def test_an_email_action_names_its_body_template
    assert_equal({ "type" => "email", "to" => "ops@example.com", "subject" => "Order deleted",
                   "body_template" => "Order {{ id }}" },
                 action_of { |o| o.email "ops@example.com", "Order deleted", "Order {{ id }}" })
  end

  def test_a_condition_is_emitted_and_an_unset_retry_is_left_out
    schema = FraiseQL::Schema.new
    schema.observer :order_shipped, entity: "Order", event: "UPDATE",
                                    condition: "status == 'shipped'" do |o|
      o.email "ops@example.com", "s", "b"
    end
    definition = schema.to_h["observers"].first
    assert_equal "status == 'shipped'", definition["condition"]
    # No retry: the server's own policy applies, so no block is written.
    refute definition.key?("retry")
  end

  def test_no_observer_leaves_the_section_out
    refute FraiseQL::Schema.new.to_h.key?("observers")
  end

  def test_retry_is_an_observer_retry_and_nothing_else
    error = assert_raises(TypeError) do
      FraiseQL::Schema.new.observer(:order_created, entity: "Order", event: "INSERT",
                                                    retry_: { max_attempts: 3 }) do |o|
        o.email "a@example.com", "s", "b"
      end
    end
    assert_includes error.message, "FraiseQL::ObserverRetry"
  end

  def test_an_observer_name_is_registered_once
    schema = FraiseQL::Schema.new
    declare = lambda do
      schema.observer(:order_created, entity: "Order", event: "INSERT") { |o| o.email "a@example.com", "s", "b" }
    end
    declare.call
    error = assert_raises(ArgumentError) { declare.call }
    assert_includes error.message, "observer \"orderCreated\" is already declared"
  end
end
