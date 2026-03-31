defmodule FraiseQL.CrudGenerator do
  @moduledoc """
  Generates CRUD queries and mutations for FraiseQL types.

  When a `FraiseQL.TypeDefinition` has `crud: true` (or a list of specific
  operations like `[:read, :create, :update, :delete]`), this module generates
  the standard queries and mutations following FraiseQL conventions:

    * **Read**: query `{snake}` (get by PK) + query `{snakes}` (list with auto_params)
    * **Create**: mutation `create_{snake}` with all fields, sql_source `fn_create_{snake}`, operation INSERT
    * **Update**: mutation `update_{snake}` with PK required + other fields nullable, sql_source `fn_update_{snake}`, operation UPDATE
    * **Delete**: mutation `delete_{snake}` with PK only, sql_source `fn_delete_{snake}`, operation DELETE
  """

  alias FraiseQL.{QueryDefinition, MutationDefinition, ArgumentDefinition}

  @doc """
  Generate CRUD operations from a `FraiseQL.TypeDefinition`.

  Returns `{queries, mutations}` where each is a list of the corresponding
  definition structs.

  ## Options

    * `:cascade` — when `true`, generated mutations include `cascade: true` (default `false`)

  ## Errors

  Raises `ArgumentError` if the type has no fields.
  """
  @spec generate(FraiseQL.TypeDefinition.t(), keyword()) ::
          {[QueryDefinition.t()], [MutationDefinition.t()]}
  def generate(%FraiseQL.TypeDefinition{} = type, opts \\ []) do
    cascade = Keyword.get(opts, :cascade, false)
    ops = parse_crud_ops(type.crud)

    if Enum.empty?(ops) do
      {[], []}
    else
      do_generate(type, ops, cascade)
    end
  end

  defp do_generate(type, ops, cascade) do
    if type.fields == [] do
      raise ArgumentError,
            "type #{inspect(type.name)} has no fields; cannot generate CRUD operations"
    end

    snake = pascal_to_snake(type.name)
    view = type.sql_source
    pk_field = List.first(type.fields)

    queries =
      if :read in ops do
        generate_read_ops(type.name, snake, view, pk_field)
      else
        []
      end

    mutations =
      List.flatten([
        if(:create in ops, do: [generate_create_op(type.name, snake, type.fields, cascade)], else: []),
        if(:update in ops, do: [generate_update_op(type.name, snake, pk_field, type.fields, cascade)], else: []),
        if(:delete in ops, do: [generate_delete_op(type.name, snake, pk_field, cascade)], else: [])
      ])

    {queries, mutations}
  end

  defp parse_crud_ops(true), do: [:read, :create, :update, :delete]
  defp parse_crud_ops(false), do: []
  defp parse_crud_ops(ops) when is_list(ops), do: ops
  defp parse_crud_ops(_other), do: []

  defp generate_read_ops(type_name, snake, view, pk_field) do
    get_by_id = %QueryDefinition{
      name: snake,
      return_type: type_name,
      sql_source: view,
      returns_list: false,
      nullable: true,
      arguments: [
        %ArgumentDefinition{name: pk_field.name, type: pk_field.type, nullable: false}
      ],
      description: "Get #{type_name} by ID."
    }

    list = %QueryDefinition{
      name: pluralize(snake),
      return_type: type_name,
      sql_source: view,
      returns_list: true,
      nullable: false,
      arguments: [],
      description: "List #{type_name} records.",
      auto_params: %{where: true, order_by: true, limit: true, offset: true}
    }

    [get_by_id, list]
  end

  defp generate_create_op(type_name, snake, fields, cascade) do
    args =
      Enum.map(fields, fn f ->
        %ArgumentDefinition{name: f.name, type: f.type, nullable: f.nullable}
      end)

    %MutationDefinition{
      name: "create_#{snake}",
      return_type: type_name,
      sql_source: "fn_create_#{snake}",
      operation: "INSERT",
      arguments: args,
      description: "Create a new #{type_name}.",
      cascade: cascade
    }
  end

  defp generate_update_op(type_name, snake, pk_field, fields, cascade) do
    pk_arg = %ArgumentDefinition{name: pk_field.name, type: pk_field.type, nullable: false}

    other_args =
      fields
      |> Enum.drop(1)
      |> Enum.map(fn f ->
        %ArgumentDefinition{name: f.name, type: f.type, nullable: true}
      end)

    %MutationDefinition{
      name: "update_#{snake}",
      return_type: type_name,
      sql_source: "fn_update_#{snake}",
      operation: "UPDATE",
      arguments: [pk_arg | other_args],
      description: "Update an existing #{type_name}.",
      cascade: cascade
    }
  end

  defp generate_delete_op(type_name, snake, pk_field, cascade) do
    %MutationDefinition{
      name: "delete_#{snake}",
      return_type: type_name,
      sql_source: "fn_delete_#{snake}",
      operation: "DELETE",
      arguments: [
        %ArgumentDefinition{name: pk_field.name, type: pk_field.type, nullable: false}
      ],
      description: "Delete a #{type_name}.",
      cascade: cascade
    }
  end

  @doc """
  Converts a PascalCase name to snake_case.

  ## Examples

      iex> FraiseQL.CrudGenerator.pascal_to_snake("BlogPost")
      "blog_post"

      iex> FraiseQL.CrudGenerator.pascal_to_snake("User")
      "user"
  """
  @spec pascal_to_snake(String.t()) :: String.t()
  def pascal_to_snake(name) do
    name
    |> String.replace(~r/(?<!^)([A-Z])/, "_\\1")
    |> String.downcase()
  end

  @doc """
  Applies basic English pluralization rules to a snake_case name.

  Rules (ordered):
    1. Already ends in 's' (but not 'ss') -> no change (e.g. 'statistics')
    2. Ends in 'ss', 'sh', 'ch', 'x', 'z' -> append 'es'
    3. Ends in consonant + 'y' -> replace 'y' with 'ies'
    4. Default -> append 's'

  ## Examples

      iex> FraiseQL.CrudGenerator.pluralize("author")
      "authors"

      iex> FraiseQL.CrudGenerator.pluralize("address")
      "addresses"

      iex> FraiseQL.CrudGenerator.pluralize("category")
      "categories"
  """
  @spec pluralize(String.t()) :: String.t()
  def pluralize(name) do
    cond do
      String.ends_with?(name, "s") and not String.ends_with?(name, "ss") ->
        name

      Enum.any?(["ss", "sh", "ch", "x", "z"], &String.ends_with?(name, &1)) ->
        name <> "es"

      String.length(name) >= 2 and String.ends_with?(name, "y") and
          String.at(name, String.length(name) - 2) not in ~w(a e i o u) ->
        String.slice(name, 0..-2//1) <> "ies"

      true ->
        name <> "s"
    end
  end
end
