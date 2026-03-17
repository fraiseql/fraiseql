defmodule FraiseQL.GraphQLError do
  @moduledoc "One or more errors from the GraphQL errors array."
  @enforce_keys [:errors, :message]
  defstruct [:errors, :message]

  @type error_entry :: %{
          required(:message) => String.t(),
          optional(:locations) => list(%{line: integer(), column: integer()}),
          optional(:path) => list(String.t() | integer()),
          optional(:extensions) => map()
        }

  @type t :: %__MODULE__{
          errors: [error_entry()],
          message: String.t()
        }

  @spec new([error_entry()]) :: t()
  def new([first | _] = errors),
    do: %__MODULE__{
      errors: errors,
      message: Map.get(first, :message, Map.get(first, "message", "GraphQL error"))
    }

  def new([]), do: %__MODULE__{errors: [], message: "GraphQL error"}
end

defmodule FraiseQL.NetworkError do
  @moduledoc "Transport-level error."
  defstruct [:reason, :message]
  @type t :: %__MODULE__{reason: term(), message: String.t()}
end

defmodule FraiseQL.TimeoutError do
  @moduledoc "Request timeout error."
  defstruct [:timeout_ms, :message]
  @type t :: %__MODULE__{timeout_ms: pos_integer() | nil, message: String.t()}
end

defmodule FraiseQL.AuthenticationError do
  @moduledoc "401/403 authentication error."
  defstruct [:status_code, :message]
  @type t :: %__MODULE__{status_code: 401 | 403, message: String.t()}
end
