defmodule FraiseQL.CrudGenerator do
  @moduledoc """
  Generates CRUD queries and mutations for FraiseQL types.

  When a `FraiseQL.TypeDefinition` has `crud: true` (or a list of specific
  operations like `[:read, :create, :update, :delete]`), this module generates
  the standard queries and mutations following FraiseQL conventions:

    * **Read**: query `{snake}` (get by PK) + query `{snakes}` (list with auto_params)
    * **Create**: mutation `create_{snake}` taking `input: Create{Type}Input!`, sql_source `fn_create_{snake}`, operation INSERT
    * **Update**: mutation `update_{snake}` taking `input: Update{Type}Input!`, sql_source `fn_update_{snake}`, operation UPDATE
    * **Delete**: mutation `delete_{snake}` with PK only, sql_source `fn_delete_{snake}`, operation DELETE

  The two input objects are the shape six of the nine generating SDKs emit and the one
  `docs/architecture/mutation-response.md` documents. This generator emitted flat arguments
  and no input types, so the same `crud:` declaration produced a different GraphQL API in
  Elixir than in Python (#1246). They are returned as `is_input` `TypeDefinition`s, which is
  this SDK's only route to an input object — the exporter emits no `input_types` key.
  """

  alias FraiseQL.{QueryDefinition, MutationDefinition, ArgumentDefinition, TypeDefinition, FieldDefinition}

  @doc """
  Generate CRUD operations from a `FraiseQL.TypeDefinition`.

  Returns `{queries, mutations, input_types}` where each is a list of the corresponding
  definition structs. The input types are `is_input` `TypeDefinition`s — this SDK's only
  route to an input object, since the exporter emits no `input_types` key.

  ## Options

    * `:cascade` — when `true`, generated mutations include `cascade: true` (default `false`)

  ## Errors

  Raises `ArgumentError` if the type has no fields.
  """
  @spec generate(FraiseQL.TypeDefinition.t(), keyword()) ::
          {[QueryDefinition.t()], [MutationDefinition.t()], [TypeDefinition.t()]}
  def generate(%FraiseQL.TypeDefinition{} = type, opts \\ []) do
    cascade = Keyword.get(opts, :cascade, false)
    ops = parse_crud_ops(type.crud)

    if Enum.empty?(ops) do
      {[], [], []}
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
        if(:create in ops, do: [generate_create_op(type.name, snake, cascade)], else: []),
        if(:update in ops, do: [generate_update_op(type.name, snake, cascade)], else: []),
        if(:delete in ops, do: [generate_delete_op(type.name, snake, pk_field, cascade)], else: [])
      ])

    input_types =
      List.flatten([
        if(:create in ops, do: [create_input_type(type.name, type.fields)], else: []),
        if(:update in ops, do: [update_input_type(type.name, pk_field, type.fields)], else: [])
      ])

    {queries, mutations, input_types}
  end

  # A computed field is server-assigned — a slug, a view aggregation — so a client cannot
  # supply one and it is omitted from both input objects.
  defp create_input_type(type_name, fields) do
    %TypeDefinition{
      name: "Create#{type_name}Input",
      sql_source: nil,
      is_input: true,
      description: "Input for creating a new #{type_name}.",
      fields:
        fields
        |> Enum.reject(& &1.computed)
        |> Enum.map(fn f ->
          %FieldDefinition{name: f.name, type: f.type, nullable: f.nullable}
        end)
    }
  end

  defp update_input_type(type_name, pk_field, fields) do
    %TypeDefinition{
      name: "Update#{type_name}Input",
      sql_source: nil,
      is_input: true,
      description: "Input for updating an existing #{type_name}.",
      fields:
        [%FieldDefinition{name: pk_field.name, type: pk_field.type, nullable: false}] ++
          (fields
           |> Enum.drop(1)
           |> Enum.reject(& &1.computed)
           |> Enum.map(fn f ->
             %FieldDefinition{name: f.name, type: f.type, nullable: true}
           end))
    }
  end

  defp parse_crud_ops(true), do: [:read, :create, :update, :delete]
  defp parse_crud_ops(false), do: []
  defp parse_crud_ops(ops) when is_list(ops), do: ops
  defp parse_crud_ops(_other), do: []

  defp generate_read_ops(type_name, snake, view, pk_field) do
    get_by_id = %QueryDefinition{
      name: snake_to_camel(snake),
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
      name: snake_to_camel(pluralize(snake)),
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

  defp generate_create_op(type_name, snake, cascade) do
    %MutationDefinition{
      name: snake_to_camel("create_#{snake}"),
      return_type: type_name,
      sql_source: "fn_create_#{snake}",
      operation: "INSERT",
      arguments: [
        %ArgumentDefinition{name: "input", type: "Create#{type_name}Input", nullable: false}
      ],
      description: "Create a new #{type_name}.",
      cascade: cascade
    }
  end

  defp generate_update_op(type_name, snake, cascade) do
    %MutationDefinition{
      name: snake_to_camel("update_#{snake}"),
      return_type: type_name,
      sql_source: "fn_update_#{snake}",
      operation: "UPDATE",
      arguments: [
        %ArgumentDefinition{name: "input", type: "Update#{type_name}Input", nullable: false}
      ],
      description: "Update an existing #{type_name}.",
      cascade: cascade
    }
  end

  defp generate_delete_op(type_name, snake, pk_field, cascade) do
    %MutationDefinition{
      name: snake_to_camel("delete_#{snake}"),
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
  Convert a snake_case name to camelCase. Idempotent.

  The generated operations carried the snake_case name verbatim, so a `crud: true` type
  produced `create_support_ticket` in a schema whose hand-authored mutations beside it
  were `createUser` — one SDK emitting two naming conventions, and a different GraphQL API
  from the one Python generates for the same declaration (#1247). The compiler does not
  rename: `naming_convention` in the document is metadata, so the SDK emits the final name.
  """
  @spec snake_to_camel(String.t()) :: String.t()
  def snake_to_camel(name) do
    Regex.replace(~r/_([a-z])/, name, fn _, c -> String.upcase(c) end)
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
