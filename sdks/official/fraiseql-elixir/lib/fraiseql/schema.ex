defmodule FraiseQL.Schema do
  @moduledoc """
  Compile-time DSL for authoring FraiseQL schemas.

  Use this module in any Elixir module to declare types, queries, and mutations
  that will be compiled into a `schema.json` file consumed by `fraiseql compile`.

  ## Quick Start

      defmodule MyApp.Schema do
        use FraiseQL.Schema

        fraiseql_type "Author", sql_source: "v_author", description: "A blog author" do
          field :id,   :id,     nullable: false
          field :name, :string, nullable: false
          field :bio,  :string, nullable: true
        end

        fraiseql_query :authors,
          return_type: "Author",
          returns_list: true,
          sql_source: "v_author"

        fraiseql_query :author, return_type: "Author", sql_source: "v_author" do
          argument :id, :id, nullable: false
        end

        fraiseql_mutation :create_author,
          return_type: "Author",
          sql_source: "fn_create_author",
          operation: "insert" do
          argument :name, :string, nullable: false
        end
      end

  Then export:

      MyApp.Schema.export_to_file!("schema.json")
      # or: mix fraiseql.export --module MyApp.Schema

  ## Macros

  | Macro | Purpose |
  |-------|---------|
  | `fraiseql_type/2` | Register a type (optional `do` block with `field/3` calls) |
  | `fraiseql_query/2` | Register a query (optional `do` block with `argument/3` calls) |
  | `fraiseql_mutation/2` | Register a mutation (optional `do` block with `argument/3` calls) |
  | `field/3` | Declare a field inside a `fraiseql_type` block |
  | `argument/3` | Declare an argument inside a query/mutation block |
  """

  @doc false
  defmacro __using__(_opts) do
    quote do
      import FraiseQL.Schema,
        only: [
          fraiseql_type: 2,
          fraiseql_query: 2,
          fraiseql_mutation: 2
        ]

      Module.register_attribute(__MODULE__, :fraiseql_types, accumulate: true)
      Module.register_attribute(__MODULE__, :fraiseql_queries, accumulate: true)
      Module.register_attribute(__MODULE__, :fraiseql_mutations, accumulate: true)
      Module.put_attribute(__MODULE__, :fraiseql_schema, true)
      @before_compile FraiseQL.Schema
    end
  end

  # ---------------------------------------------------------------------------
  # fraiseql_type
  # ---------------------------------------------------------------------------

  @doc """
  Registers a FraiseQL type.

  Use without a `do` block for a type with no fields, or with a `do` block
  containing `field/3` calls to declare fields.

  ## Options

    * `:sql_source` — (required) the underlying view/table name
    * `:description` — optional human-readable description
    * `:relay` — boolean, enables Relay pagination support (default `false`)
    * `:is_input` — boolean, marks this as a GraphQL input type (default `false`)
    * `:is_error` — boolean, marks this as a mutation error shape (default `false`)

  ## Examples

      # Without fields
      fraiseql_type "Author", sql_source: "v_author"

      # With fields
      fraiseql_type "Author", sql_source: "v_author", description: "A blog author" do
        field :id,   :id,     nullable: false
        field :name, :string, nullable: false
      end
  """
  defmacro fraiseql_type(name, opts) do
    {block, type_opts} = Keyword.pop(opts, :do)

    if block do
      quote do
        FraiseQL.Schema.__validate_type_opts__!(unquote(name), unquote(type_opts))

        Module.register_attribute(__MODULE__, :__fraiseql_field_buffer, accumulate: true)

        import FraiseQL.Schema, only: [field: 3, field: 2]
        unquote(block)

        @fraiseql_types %FraiseQL.TypeDefinition{
          name: unquote(name),
          sql_source: unquote(type_opts[:sql_source]),
          description: unquote(type_opts[:description]),
          fields: Enum.reverse(@__fraiseql_field_buffer),
          is_input: unquote(Keyword.get(type_opts, :is_input, false)),
          relay: unquote(Keyword.get(type_opts, :relay, false)),
          is_error: unquote(Keyword.get(type_opts, :is_error, false))
        }

        Module.delete_attribute(__MODULE__, :__fraiseql_field_buffer)
        Module.register_attribute(__MODULE__, :__fraiseql_field_buffer, accumulate: true)
      end
    else
      quote do
        FraiseQL.Schema.__validate_type_opts__!(unquote(name), unquote(type_opts))

        @fraiseql_types %FraiseQL.TypeDefinition{
          name: unquote(name),
          sql_source: unquote(type_opts[:sql_source]),
          description: unquote(type_opts[:description]),
          fields: [],
          is_input: unquote(Keyword.get(type_opts, :is_input, false)),
          relay: unquote(Keyword.get(type_opts, :relay, false)),
          is_error: unquote(Keyword.get(type_opts, :is_error, false))
        }
      end
    end
  end

  # ---------------------------------------------------------------------------
  # fraiseql_query
  # ---------------------------------------------------------------------------

  @doc """
  Registers a FraiseQL query.

  Use without a `do` block for a query with no arguments, or with a `do` block
  containing `argument/3` calls to declare arguments.

  The query name atom is used as-is as a string for the GraphQL field name,
  e.g. `:authors` → `"authors"`.

  ## Options

    * `:return_type` — (required) the GraphQL return type name string
    * `:sql_source` — (required) the underlying view/table name
    * `:returns_list` — boolean (default `false`)
    * `:nullable` — boolean (default `false`)
    * `:cache_ttl_seconds` — optional integer TTL for caching
    * `:description` — optional human-readable description

  ## Examples

      # Without arguments
      fraiseql_query :authors,
        return_type: "Author",
        returns_list: true,
        sql_source: "v_author"

      # With arguments
      fraiseql_query :author, return_type: "Author", sql_source: "v_author" do
        argument :id, :id, nullable: false
      end
  """
  defmacro fraiseql_query(name, opts) do
    {block, query_opts} = Keyword.pop(opts, :do)
    query_name = Atom.to_string(name)

    if block do
      quote do
        Module.register_attribute(__MODULE__, :__fraiseql_arg_buffer, accumulate: true)

        import FraiseQL.Schema, only: [argument: 3, argument: 2]
        unquote(block)

        @fraiseql_queries %FraiseQL.QueryDefinition{
          name: unquote(query_name),
          return_type: unquote(query_opts[:return_type]),
          sql_source: unquote(query_opts[:sql_source]),
          returns_list: unquote(Keyword.get(query_opts, :returns_list, false)),
          nullable: unquote(Keyword.get(query_opts, :nullable, false)),
          arguments: Enum.reverse(@__fraiseql_arg_buffer),
          cache_ttl_seconds: unquote(query_opts[:cache_ttl_seconds]),
          description: unquote(query_opts[:description])
        }

        Module.delete_attribute(__MODULE__, :__fraiseql_arg_buffer)
        Module.register_attribute(__MODULE__, :__fraiseql_arg_buffer, accumulate: true)
      end
    else
      quote do
        @fraiseql_queries %FraiseQL.QueryDefinition{
          name: unquote(query_name),
          return_type: unquote(query_opts[:return_type]),
          sql_source: unquote(query_opts[:sql_source]),
          returns_list: unquote(Keyword.get(query_opts, :returns_list, false)),
          nullable: unquote(Keyword.get(query_opts, :nullable, false)),
          arguments: [],
          cache_ttl_seconds: unquote(query_opts[:cache_ttl_seconds]),
          description: unquote(query_opts[:description])
        }
      end
    end
  end

  # ---------------------------------------------------------------------------
  # fraiseql_mutation
  # ---------------------------------------------------------------------------

  @doc """
  Registers a FraiseQL mutation.

  Use without a `do` block for a mutation with no arguments, or with a `do` block
  containing `argument/3` calls to declare arguments.

  The mutation name atom is converted to camelCase string automatically,
  e.g. `:create_author` → `"createAuthor"`.

  ## Options

    * `:return_type` — (required) the GraphQL return type name string
    * `:sql_source` — (required) the underlying function name
    * `:operation` — (required) one of `"insert"`, `"update"`, `"delete"`
    * `:description` — optional human-readable description

  ## Examples

      # Without arguments
      fraiseql_mutation :delete_author,
        return_type: "Author",
        sql_source: "fn_delete_author",
        operation: "delete"

      # With arguments
      fraiseql_mutation :create_author,
        return_type: "Author",
        sql_source: "fn_create_author",
        operation: "insert" do
        argument :name, :string, nullable: false
        argument :bio,  :string, nullable: true
      end
  """
  defmacro fraiseql_mutation(name, opts) do
    {block, mutation_opts} = Keyword.pop(opts, :do)
    mutation_name = FraiseQL.TypeMapper.to_camel_case(name)

    if block do
      quote do
        Module.register_attribute(__MODULE__, :__fraiseql_arg_buffer, accumulate: true)

        import FraiseQL.Schema, only: [argument: 3, argument: 2]
        unquote(block)

        @fraiseql_mutations %FraiseQL.MutationDefinition{
          name: unquote(mutation_name),
          return_type: unquote(mutation_opts[:return_type]),
          sql_source: unquote(mutation_opts[:sql_source]),
          operation: unquote(mutation_opts[:operation]),
          arguments: Enum.reverse(@__fraiseql_arg_buffer),
          description: unquote(mutation_opts[:description])
        }

        Module.delete_attribute(__MODULE__, :__fraiseql_arg_buffer)
        Module.register_attribute(__MODULE__, :__fraiseql_arg_buffer, accumulate: true)
      end
    else
      quote do
        @fraiseql_mutations %FraiseQL.MutationDefinition{
          name: unquote(mutation_name),
          return_type: unquote(mutation_opts[:return_type]),
          sql_source: unquote(mutation_opts[:sql_source]),
          operation: unquote(mutation_opts[:operation]),
          arguments: [],
          description: unquote(mutation_opts[:description])
        }
      end
    end
  end

  # ---------------------------------------------------------------------------
  # field and argument helper macros
  # ---------------------------------------------------------------------------

  @doc """
  Declares a field inside a `fraiseql_type` block.

  ## Options

    * `:nullable` — boolean (default `false`)
    * `:description` — optional human-readable description
    * `:requires_scope` — optional OAuth scope string
    * `:requires_scopes` — optional list of OAuth scope strings

  ## Example

      field :name, :string, nullable: false
      field :email, :string, nullable: true, requires_scope: "read:user.email"
  """
  defmacro field(name, type, opts \\ []) do
    field_name = Atom.to_string(name)
    field_type = FraiseQL.TypeMapper.to_graphql_type(type)

    quote do
      @__fraiseql_field_buffer %FraiseQL.FieldDefinition{
        name: unquote(field_name),
        type: unquote(field_type),
        nullable: unquote(Keyword.get(opts, :nullable, false)),
        description: unquote(opts[:description]),
        requires_scope: unquote(opts[:requires_scope]),
        requires_scopes: unquote(opts[:requires_scopes])
      }
    end
  end

  @doc """
  Declares an argument inside a `fraiseql_query` or `fraiseql_mutation` block.

  ## Options

    * `:nullable` — boolean (default `false`)
    * `:description` — optional human-readable description

  ## Example

      argument :id, :id, nullable: false
      argument :limit, :integer, nullable: true
  """
  defmacro argument(name, type, opts \\ []) do
    arg_name = Atom.to_string(name)
    arg_type = FraiseQL.TypeMapper.to_graphql_type(type)

    quote do
      @__fraiseql_arg_buffer %FraiseQL.ArgumentDefinition{
        name: unquote(arg_name),
        type: unquote(arg_type),
        nullable: unquote(Keyword.get(opts, :nullable, false)),
        description: unquote(opts[:description])
      }
    end
  end

  # ---------------------------------------------------------------------------
  # Validation helpers (called at compile time from macro expansions)
  # ---------------------------------------------------------------------------

  @doc false
  @spec __validate_type_opts__!(String.t(), keyword()) :: :ok
  def __validate_type_opts__!(name, opts) do
    unless Keyword.has_key?(opts, :sql_source) and not is_nil(opts[:sql_source]) do
      raise ArgumentError,
            "fraiseql_type #{inspect(name)}: sql_source is required. " <>
              "Example: fraiseql_type #{inspect(name)}, sql_source: \"v_#{String.downcase(name)}\""
    end

    :ok
  end

  # ---------------------------------------------------------------------------
  # @before_compile — inject accessor functions
  # ---------------------------------------------------------------------------

  @doc false
  defmacro __before_compile__(env) do
    types = Module.get_attribute(env.module, :fraiseql_types) |> Enum.reverse()
    queries = Module.get_attribute(env.module, :fraiseql_queries) |> Enum.reverse()
    mutations = Module.get_attribute(env.module, :fraiseql_mutations) |> Enum.reverse()

    validate_no_duplicate_types!(types, env.module)

    quote do
      @doc """
      Returns all type definitions declared in this schema module, in declaration order.
      """
      @spec __fraiseql_types__() :: [FraiseQL.TypeDefinition.t()]
      def __fraiseql_types__, do: unquote(Macro.escape(types))

      @doc """
      Returns all query definitions declared in this schema module, in declaration order.
      """
      @spec __fraiseql_queries__() :: [FraiseQL.QueryDefinition.t()]
      def __fraiseql_queries__, do: unquote(Macro.escape(queries))

      @doc """
      Returns all mutation definitions declared in this schema module, in declaration order.
      """
      @spec __fraiseql_mutations__() :: [FraiseQL.MutationDefinition.t()]
      def __fraiseql_mutations__, do: unquote(Macro.escape(mutations))

      @doc """
      Exports this schema to a JSON file at `path`.

      Equivalent to `FraiseQL.SchemaExporter.export_to_file!(__MODULE__, path, opts)`.
      """
      @spec export_to_file!(Path.t(), keyword()) :: :ok
      def export_to_file!(path, opts \\ []) do
        FraiseQL.SchemaExporter.export_to_file!(__MODULE__, path, opts)
      end

      @doc """
      Converts this schema module to a `%FraiseQL.IntermediateSchema{}` struct.

      Equivalent to `FraiseQL.SchemaExporter.to_intermediate_schema(__MODULE__)`.
      """
      @spec to_intermediate_schema() :: FraiseQL.IntermediateSchema.t()
      def to_intermediate_schema do
        FraiseQL.SchemaExporter.to_intermediate_schema(__MODULE__)
      end
    end
  end

  defp validate_no_duplicate_types!(types, module) do
    names = Enum.map(types, & &1.name)
    duplicates = names -- Enum.uniq(names)

    unless Enum.empty?(duplicates) do
      dup = List.first(duplicates)

      raise ArgumentError,
            "duplicate type name #{inspect(dup)} in #{inspect(module)}. " <>
              "Each type name must be unique within a schema module."
    end
  end
end
