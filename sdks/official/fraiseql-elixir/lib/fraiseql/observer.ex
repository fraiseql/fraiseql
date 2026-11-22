defmodule FraiseQL.RetryConfig do
  @moduledoc """
  How an observer retries an action that fails (#2147). An observer given none runs on
  the server's own policy.

    * `:max_attempts` — the attempts, the first included
    * `:backoff_strategy` — `"fixed"`, `"linear"` or `"exponential"`
    * `:initial_delay_ms` — the delay before the first retry
    * `:max_delay_ms` — the longest delay between retries
  """

  @enforce_keys [:max_attempts, :backoff_strategy, :initial_delay_ms, :max_delay_ms]
  defstruct @enforce_keys

  @type t :: %__MODULE__{
          max_attempts: pos_integer(),
          backoff_strategy: String.t(),
          initial_delay_ms: non_neg_integer(),
          max_delay_ms: non_neg_integer()
        }
end

defmodule FraiseQL.ObserverDefinition do
  @moduledoc """
  An observer (#2147): actions run when a row of `:entity` sees `:event` (`"INSERT"`,
  `"UPDATE"` or `"DELETE"`), when `:condition`, an expression on the row, holds.
  `:actions` are maps of the keys the server's observer runtime reads; `:retry` is `nil`
  for the server's own policy.
  """

  defstruct [:name, :entity, :event, :condition, :retry, actions: []]

  @type t :: %__MODULE__{
          name: String.t(),
          entity: String.t(),
          event: String.t(),
          condition: String.t() | nil,
          actions: [map()],
          retry: FraiseQL.RetryConfig.t() | nil
        }
end

defmodule FraiseQL.Observer do
  @moduledoc """
  The actions an observer runs (#2147): a webhook, a Slack message or an email, with the
  keys the server's observer runtime reads. `fraiseql compile` checks them against it.

      fraiseql_observer :order_shipped,
        entity: "Order",
        event: "UPDATE",
        condition: "status == 'shipped'",
        actions: [FraiseQL.Observer.slack("#orders", "Order {{ id }} shipped")]
  """

  @default_slack_webhook_env "SLACK_WEBHOOK_URL"

  @doc "The variable a Slack action reads its incoming-webhook URL from, unless told another."
  @spec default_slack_webhook_env() :: String.t()
  def default_slack_webhook_env, do: @default_slack_webhook_env

  @doc """
  An HTTP request to `:url`, or to the URL the server reads from `:url_env` at delivery;
  exactly one of the two. Optional: `:method`, `:body_template` (a `{{ field }}`
  template), `:signing_secret_env` (the variable holding the key the request is signed
  with), `:headers` and `:headers_env` (a header to the variable holding its value).

  Raises `ArgumentError` when neither or both of `:url` and `:url_env` are given.
  """
  @spec webhook(keyword()) :: map()
  def webhook(opts) do
    if is_nil(opts[:url]) == is_nil(opts[:url_env]) do
      raise ArgumentError,
            "FraiseQL.Observer.webhook: give exactly one of url: and url_env: (the destination)."
    end

    scalars =
      for key <- [:url, :url_env, :method, :body_template, :signing_secret_env],
          value = opts[key],
          not is_nil(value),
          into: %{},
          do: {Atom.to_string(key), value}

    maps =
      for key <- [:headers, :headers_env],
          value = opts[key],
          is_map(value) and map_size(value) > 0,
          into: %{},
          do: {Atom.to_string(key), Map.new(value, fn {k, v} -> {to_string(k), v} end)}

    %{"type" => "webhook"} |> Map.merge(scalars) |> Map.merge(maps)
  end

  @doc """
  Posts `message`, a `{{ field }}` template, to `channel`, at the URL in
  `SLACK_WEBHOOK_URL` unless `:webhook_url` (written into the schema) or
  `:webhook_url_env` names another.

  Raises `ArgumentError` when both `:webhook_url` and `:webhook_url_env` are given.
  """
  @spec slack(String.t(), String.t(), keyword()) :: map()
  def slack(channel, message, opts \\ []) do
    if opts[:webhook_url] && opts[:webhook_url_env] do
      raise ArgumentError,
            "FraiseQL.Observer.slack: give at most one of webhook_url: and webhook_url_env: " <>
              "(the destination)."
    end

    destination =
      case opts[:webhook_url] do
        nil -> %{"webhook_url_env" => opts[:webhook_url_env] || @default_slack_webhook_env}
        url -> %{"webhook_url" => url}
      end

    Map.merge(
      %{"type" => "slack", "channel" => channel, "message_template" => message},
      destination
    )
  end

  @doc """
  An email to `to`; `body` is a `{{ field }}` template. The sender is the server's SMTP
  configuration: an action cannot set one.
  """
  @spec email(String.t(), String.t(), String.t()) :: map()
  def email(to, subject, body) do
    %{"type" => "email", "to" => to, "subject" => subject, "body_template" => body}
  end

  @doc false
  # Called from the module body `fraiseql_observer` expands into.
  @spec definition!(String.t(), keyword()) :: FraiseQL.ObserverDefinition.t()
  def definition!(name, opts) do
    retry = opts[:retry]

    unless is_nil(retry) or is_struct(retry, FraiseQL.RetryConfig) do
      raise ArgumentError,
            "fraiseql_observer #{inspect(name)}: retry: must be a %FraiseQL.RetryConfig{} " <>
              "(got #{inspect(retry)})."
    end

    %FraiseQL.ObserverDefinition{
      name: name,
      entity: Keyword.fetch!(opts, :entity),
      event: Keyword.fetch!(opts, :event),
      condition: opts[:condition],
      actions: Keyword.get(opts, :actions, []),
      retry: retry
    }
  end
end
