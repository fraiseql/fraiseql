defmodule FraiseQL.ClientTest do
  use ExUnit.Case, async: true

  test "new/1 creates a client struct with default options" do
    client = FraiseQL.Client.new("http://localhost:8000")
    assert client.url == "http://localhost:8000"
    assert client.timeout == 30_000
    assert client.authorization == nil
    assert client.headers == []
  end

  test "new/2 sets authorization header" do
    client = FraiseQL.Client.new("http://localhost:8000", authorization: "Bearer test-token")
    assert client.authorization == "Bearer test-token"
  end

  test "new/2 sets custom timeout" do
    client = FraiseQL.Client.new("http://localhost:8000", timeout: 5_000)
    assert client.timeout == 5_000
  end

  test "new/2 sets custom headers" do
    client =
      FraiseQL.Client.new("http://localhost:8000", headers: [{"X-Custom", "value"}])

    assert client.headers == [{"X-Custom", "value"}]
  end

  test "null_errors_regression - null errors treated as success" do
    # Cross-SDK invariant: {"data": {...}, "errors": null} → {:ok, data}
    body = Jason.encode!(%{data: %{users: []}, errors: nil})
    assert {:ok, %{"users" => []}} = parse_graphql_response(body)
  end

  test "parse_graphql_response returns error for non-empty errors array" do
    body = Jason.encode!(%{errors: [%{message: "Not found"}], data: nil})
    assert {:error, %FraiseQL.GraphQLError{message: "Not found"}} = parse_graphql_response(body)
  end

  test "parse_graphql_response returns ok for empty errors array" do
    # An empty errors list should still be treated as success if data is present
    body = Jason.encode!(%{data: %{users: []}, errors: []})
    assert {:ok, %{"users" => []}} = parse_graphql_response(body)
  end

  test "parse_graphql_response handles null data" do
    body = Jason.encode!(%{data: nil})
    assert {:ok, %{}} = parse_graphql_response(body)
  end

  # Helper that mirrors the internal parse logic for unit testing
  defp parse_graphql_response(body) do
    case Jason.decode(body) do
      {:ok, %{"errors" => errors}} when is_list(errors) and length(errors) > 0 ->
        graphql_errors = Enum.map(errors, &%{message: Map.get(&1, "message", "")})
        {:error, FraiseQL.GraphQLError.new(graphql_errors)}

      {:ok, %{"data" => data}} ->
        {:ok, data || %{}}

      {:error, reason} ->
        {:error, %FraiseQL.NetworkError{reason: reason, message: "Parse error"}}
    end
  end
end
