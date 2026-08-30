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
  Converts a snake_case atom or string to camelCase.

  The rule is the engine's `to_camel_case` (`fraiseql-core/src/utils/casing.rs`) exactly:
  drop each underscore and upcase the single character that follows it, leaving every
  other character alone. A name with no underscore is already camelCase and is returned
  unchanged.

  Two properties matter and neither is obvious:

    * a digit segment collapses onto the previous word — `:phone_1` is `"phone1"` and
      `:dns_1_id` is `"dns1Id"`, which a `~r/_([a-z])/` implementation silently does not
      do;
    * the rest of a segment keeps its case. This used to be `String.split("_")` plus
      `String.capitalize/1`, which *downcases* the tail, so `:user_ID` became `"userId"`
      where the engine says `"userID"`.

  ## Examples

      iex> FraiseQL.TypeMapper.to_camel_case(:create_author)
      "createAuthor"

      iex> FraiseQL.TypeMapper.to_camel_case(:author)
      "author"

      iex> FraiseQL.TypeMapper.to_camel_case(:get_user_by_id)
      "getUserById"

      iex> FraiseQL.TypeMapper.to_camel_case(:phone_1)
      "phone1"

      iex> FraiseQL.TypeMapper.to_camel_case(:dns_1_id)
      "dns1Id"

      iex> FraiseQL.TypeMapper.to_camel_case("alreadyCamel")
      "alreadyCamel"
  """
  @spec to_camel_case(atom() | String.t()) :: String.t()
  def to_camel_case(atom) when is_atom(atom), do: atom |> Atom.to_string() |> to_camel_case()

  def to_camel_case(string) when is_binary(string) do
    if String.contains?(string, "_") do
      string
      |> String.graphemes()
      |> Enum.reduce({[], false}, fn
        "_", {acc, _upcase_next} -> {acc, true}
        grapheme, {acc, true} -> {[String.upcase(grapheme) | acc], false}
        grapheme, {acc, false} -> {[grapheme | acc], false}
      end)
      |> elem(0)
      |> Enum.reverse()
      |> Enum.join()
    else
      string
    end
  end
end
