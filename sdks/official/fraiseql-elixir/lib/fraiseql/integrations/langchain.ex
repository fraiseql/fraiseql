defmodule FraiseQL.LangChain.Tool do
  @moduledoc """
  LangChain Elixir tool wrapper for FraiseQL queries.
  Requires the `langchain` package (brainlid/langchain).

  ## Usage

      tool = FraiseQL.LangChain.Tool.new(client,
        name: "get_users",
        description: "Fetch users from the FraiseQL API",
        query: "query GetUsers($limit: Int) { users(limit: $limit) { id name } }",
        parameters_schema: %{
          type: "object",
          properties: %{
            limit: %{type: "integer", description: "Maximum number of users to return"}
          }
        }
      )
  """

  @spec new(FraiseQL.Client.t(), keyword()) :: map()
  def new(%FraiseQL.Client{} = client, opts) do
    name = Keyword.fetch!(opts, :name)
    description = Keyword.fetch!(opts, :description)
    query = Keyword.fetch!(opts, :query)
    parameters_schema = Keyword.get(opts, :parameters_schema, %{})

    %{
      name: name,
      description: description,
      parameters_schema: parameters_schema,
      function: fn args, _context ->
        case FraiseQL.Client.query(client, query, variables: args) do
          {:ok, data} -> {:ok, Jason.encode!(data)}
          {:error, error} -> {:error, inspect(error)}
        end
      end
    }
  end
end
