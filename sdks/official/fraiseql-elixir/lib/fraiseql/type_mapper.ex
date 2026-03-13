defmodule FraiseQL.TypeMapper do
  @moduledoc """
  Converts Elixir type atoms to GraphQL type strings and handles name-case conversions.

  ## Known type mappings

  | Elixir atom  | GraphQL type |
  |--------------|-------------|
  | `:integer`   | `"Int"`     |
  | `:int`       | `"Int"`     |
  | `:float`     | `"Float"`   |
  | `:boolean`   | `"Boolean"` |
  | `:bool`      | `"Boolean"` |
  | `:string`    | `"String"`  |
  | `:id`        | `"ID"`      |
  | `:datetime`  | `"DateTime"`|

  Unknown atoms are converted to PascalCase, e.g. `:user_profile` → `"UserProfile"`.

  ## Examples

      iex> FraiseQL.TypeMapper.to_graphql_type(:string)
      "String"

      iex> FraiseQL.TypeMapper.to_graphql_type(:user_profile)
      "UserProfile"

      iex> FraiseQL.TypeMapper.to_camel_case(:create_author)
      "createAuthor"

      iex> FraiseQL.TypeMapper.to_pascal_case(:user_profile)
      "UserProfile"
  """

  @known_types %{
    integer: "Int",
    int: "Int",
    float: "Float",
    boolean: "Boolean",
    bool: "Boolean",
    string: "String",
    id: "ID",
    datetime: "DateTime"
  }

  @doc """
  Maps an Elixir type atom to its GraphQL type string.

  Known atoms (`:integer`, `:int`, `:float`, `:boolean`, `:bool`, `:string`,
  `:id`, `:datetime`) are mapped to their canonical GraphQL equivalents.
  All other atoms are converted to PascalCase.

  ## Examples

      iex> FraiseQL.TypeMapper.to_graphql_type(:integer)
      "Int"

      iex> FraiseQL.TypeMapper.to_graphql_type(:id)
      "ID"

      iex> FraiseQL.TypeMapper.to_graphql_type(:blog_post)
      "BlogPost"
  """
  @spec to_graphql_type(atom()) :: String.t()
  def to_graphql_type(atom) when is_atom(atom) do
    Map.get(@known_types, atom) || to_pascal_case(atom)
  end

  @doc """
  Converts an atom to PascalCase string.

  Splits on underscores and capitalises each segment.

  ## Examples

      iex> FraiseQL.TypeMapper.to_pascal_case(:user_profile)
      "UserProfile"

      iex> FraiseQL.TypeMapper.to_pascal_case(:author)
      "Author"
  """
  @spec to_pascal_case(atom()) :: String.t()
  def to_pascal_case(atom) when is_atom(atom) do
    atom
    |> Atom.to_string()
    |> String.split("_")
    |> Enum.map_join(&String.capitalize/1)
  end

  @doc """
  Converts an atom to camelCase string.

  The first segment is left lowercase; subsequent segments are capitalised.

  ## Examples

      iex> FraiseQL.TypeMapper.to_camel_case(:create_author)
      "createAuthor"

      iex> FraiseQL.TypeMapper.to_camel_case(:author)
      "author"

      iex> FraiseQL.TypeMapper.to_camel_case(:get_user_by_id)
      "getUserById"
  """
  @spec to_camel_case(atom()) :: String.t()
  def to_camel_case(atom) when is_atom(atom) do
    [first | rest] =
      atom
      |> Atom.to_string()
      |> String.split("_")

    Enum.join([first | Enum.map(rest, &String.capitalize/1)])
  end
end
