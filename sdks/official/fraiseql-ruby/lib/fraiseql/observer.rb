# frozen_string_literal: true

module FraiseQL
  # How an observer retries an action that fails (#2147). Not `FraiseQL::RetryConfig`,
  # which is the client's retry of a request. An observer given none runs on the server's
  # own policy.
  ObserverRetry = Struct.new(:max_attempts, :backoff_strategy, :initial_delay_ms, :max_delay_ms,
                             keyword_init: true) do
    def to_h
      {
        "max_attempts" => max_attempts,
        "backoff_strategy" => backoff_strategy.to_s,
        "initial_delay_ms" => initial_delay_ms,
        "max_delay_ms" => max_delay_ms
      }
    end
  end

  # Collects an observer's actions (#2147): a webhook, a Slack message or an email, with
  # the keys the server's observer runtime reads. `fraiseql compile` checks them against it.
  class ObserverActionBuilder
    # The variable a Slack action reads its incoming-webhook URL from, unless told another.
    DEFAULT_SLACK_WEBHOOK_ENV = "SLACK_WEBHOOK_URL"

    attr_reader :actions

    def initialize
      @actions = []
    end

    # An HTTP request to `url:`, or to the URL the server reads from `url_env:` at
    # delivery; exactly one of the two. `body_template:` is a `{{ field }}` template,
    # `signing_secret_env:` the variable holding the key the request is signed with, and
    # `headers_env:` maps a header to the variable holding its value.
    def webhook(url: nil, url_env: nil, method: nil, body_template: nil, signing_secret_env: nil,
                headers: {}, headers_env: {})
      if url.nil? == url_env.nil?
        raise ArgumentError, "webhook: give exactly one of url: and url_env: (the destination)."
      end

      action = { "type" => "webhook" }
      {
        "url" => url, "url_env" => url_env, "method" => method, "body_template" => body_template,
        "signing_secret_env" => signing_secret_env
      }.each { |key, value| action[key] = value.to_s unless value.nil? }
      action["headers"] = stringify(headers) unless headers.empty?
      action["headers_env"] = stringify(headers_env) unless headers_env.empty?
      add(action)
    end

    # Posts `message`, a `{{ field }}` template, to `channel`, at the URL in
    # `DEFAULT_SLACK_WEBHOOK_ENV` unless `webhook_url:` (written into the schema) or
    # `webhook_url_env:` names another.
    def slack(channel, message, webhook_url: nil, webhook_url_env: nil)
      if webhook_url && webhook_url_env
        raise ArgumentError,
              "slack: give at most one of webhook_url: and webhook_url_env: (the destination)."
      end

      action = { "type" => "slack", "channel" => channel.to_s, "message_template" => message.to_s }
      if webhook_url
        action["webhook_url"] = webhook_url.to_s
      else
        action["webhook_url_env"] = (webhook_url_env || DEFAULT_SLACK_WEBHOOK_ENV).to_s
      end
      add(action)
    end

    # An email to `to`; `body` is a `{{ field }}` template. The sender is the server's SMTP
    # configuration: an action cannot set one.
    def email(to, subject, body)
      add({ "type" => "email", "to" => to.to_s, "subject" => subject.to_s, "body_template" => body.to_s })
    end

    private

    def add(action)
      @actions << action
      action
    end

    def stringify(map)
      map.to_h { |key, value| [key.to_s, value.to_s] }
    end
  end
end
