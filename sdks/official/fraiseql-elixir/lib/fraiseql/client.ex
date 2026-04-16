defmodule FraiseQL.Client do
  @moduledoc """
  HTTP client for executing GraphQL queries against a FraiseQL server.

  ## Usage

      client = FraiseQL.Client.new("http://localhost:8000")
      {:ok, data} = FraiseQL.Client.query(client, "{ users { id name } }")
      {:ok, data} = FraiseQL.Client.query(client, "query($id: Int!) { user(id: $id) { name } }", variables: %{id: 42})

  ## Error handling

      case FraiseQL.Client.query(client, "{ secret }") do
        {:ok, data} -> data
        {:error, %FraiseQL.GraphQLError{} = err} -> handle_graphql_error(err)
        {:error, %FraiseQL.NetworkError{} = err} -> handle_network_error(err)
        {:error, %FraiseQL.AuthenticationError{}} -> handle_auth_error()
      end
  """

  # Suppress Dialyzer warnings for Erlang :httpc.request/4 which has incomplete type stubs
  @dialyzer {:nowarn_function, execute: 4}

  defstruct [:url, :authorization, :timeout, :retry, :headers]

  @type t :: %__MODULE__{
          url: String.t(),
          authorization: String.t() | nil,
          timeout: pos_integer(),
          retry: keyword(),
          headers: [{String.t(), String.t()}]
        }

  @type error ::
          FraiseQL.GraphQLError.t()
          | FraiseQL.NetworkError.t()
          | FraiseQL.TimeoutError.t()
          | FraiseQL.AuthenticationError.t()

  @type result :: {:ok, map()} | {:error, error()}

  @spec new(String.t(), keyword()) :: t()
  def new(url, opts \\ []) do
    %__MODULE__{
      url: url,
      authorization: Keyword.get(opts, :authorization),
      timeout: Keyword.get(opts, :timeout, 30_000),
      retry: Keyword.get(opts, :retry, max_attempts: 1),
      headers: Keyword.get(opts, :headers, [])
    }
  end

  @spec query(t(), String.t(), keyword()) :: result()
  def query(%__MODULE__{} = client, query, opts \\ []) do
    variables = Keyword.get(opts, :variables, %{})
    operation_name = Keyword.get(opts, :operation_name)
    execute(client, query, variables, operation_name)
  end

  @spec query!(t(), String.t(), keyword()) :: map() | no_return()
  def query!(%__MODULE__{} = client, query, opts \\ []) do
    case query(client, query, opts) do
      {:ok, data} -> data
      {:error, error} -> raise "FraiseQL error: #{inspect(error)}"
    end
  end

  @spec mutate(t(), String.t(), keyword()) :: result()
  def mutate(%__MODULE__{} = client, mutation, opts \\ []) do
    variables = Keyword.get(opts, :variables, %{})
    operation_name = Keyword.get(opts, :operation_name)
    execute(client, mutation, variables, operation_name)
  end

  @spec mutate!(t(), String.t(), keyword()) :: map() | no_return()
  def mutate!(%__MODULE__{} = client, mutation, opts \\ []) do
    case mutate(client, mutation, opts) do
      {:ok, data} -> data
      {:error, error} -> raise "FraiseQL error: #{inspect(error)}"
    end
  end

  defp execute(%__MODULE__{} = client, gql_query, variables, operation_name) do
    payload = %{query: gql_query, variables: variables}
    payload = if operation_name, do: Map.put(payload, :operationName, operation_name), else: payload
    body = Jason.encode!(payload)

    headers = [
      {"Content-Type", "application/json"},
      {"Accept", "application/json"}
      | build_auth_headers(client)
    ] ++ client.headers

    url = ensure_graphql_path(client.url)

    # Reason: :httpc.request/4 is valid Erlang; Dialyzer type stubs may be incomplete
    case :httpc.request(
           :post,
           {String.to_charlist(url), headers_to_charlist(headers), ~c"application/json", body},
           [{:timeout, client.timeout}],
           []
         ) do
      {:ok, {{_, 200, _}, _resp_headers, resp_body}} ->
        parse_response(resp_body)

      {:ok, {{_, 401, _}, _, _}} ->
        {:error,
         %FraiseQL.AuthenticationError{
           status_code: 401,
           message: "Authentication failed (HTTP 401)"
         }}

      {:ok, {{_, 403, _}, _, _}} ->
        {:error,
         %FraiseQL.AuthenticationError{
           status_code: 403,
           message: "Authentication failed (HTTP 403)"
         }}

      {:ok, {{_, status, _}, _, resp_body}} ->
        {:error,
         %FraiseQL.NetworkError{
           reason: {:http_error, status},
           message: "HTTP #{status}: #{resp_body}"
         }}

      {:error, {:failed_connect, _}} ->
        {:error,
         %FraiseQL.NetworkError{
           reason: :connection_refused,
           message: "Connection refused: #{url}"
         }}

      {:error, :timeout} ->
        {:error,
         %FraiseQL.TimeoutError{
           timeout_ms: client.timeout,
           message: "Request timed out after #{client.timeout}ms"
         }}

      {:error, reason} ->
        {:error,
         %FraiseQL.NetworkError{reason: reason, message: "Network error: #{inspect(reason)}"}}
    end
  end

  defp parse_response(body) do
    case Jason.decode(to_string(body)) do
      {:ok, %{"errors" => errors}} when is_list(errors) and length(errors) > 0 ->
        graphql_errors =
          Enum.map(errors, fn e ->
            %{message: Map.get(e, "message", "Unknown error")}
          end)

        {:error, FraiseQL.GraphQLError.new(graphql_errors)}

      {:ok, %{"data" => data}} ->
        # null errors = success (cross-SDK invariant)
        {:ok, data || %{}}

      {:error, reason} ->
        {:error, %FraiseQL.NetworkError{reason: reason, message: "Failed to parse response"}}
    end
  end

  defp build_auth_headers(%__MODULE__{authorization: nil}), do: []
  defp build_auth_headers(%__MODULE__{authorization: auth}), do: [{"Authorization", auth}]

  defp ensure_graphql_path(url) do
    if String.ends_with?(url, "/graphql"), do: url, else: url
  end

  defp headers_to_charlist(headers) do
    Enum.map(headers, fn {k, v} -> {String.to_charlist(k), String.to_charlist(v)} end)
  end
end
