defmodule FraiseQL.Sentinel do
  @moduledoc """
  Sentinel values for FraiseQL update inputs.

  Use `:unset` as the default value for optional update fields to distinguish
  "field not provided" from "explicitly set to nil".
  """

  @doc "The UNSET sentinel atom."
  @spec unset() :: :unset
  def unset, do: :unset

  @doc "Returns true if the value is the UNSET sentinel."
  @spec unset?(term()) :: boolean()
  def unset?(value), do: value == :unset
end

defmodule FraiseQL.FieldDefinition do
  @moduledoc """
  Represents a field on a FraiseQL type.

  ## Fields

    * `:name` — the field name as it appears in GraphQL (string)
    * `:type` — the GraphQL type string, e.g. `"ID"`, `"String"`, `"DateTime"`
    * `:nullable` — whether the field is nullable; defaults to `false`
    * `:description` — optional human-readable description
    * `:requires_scope` — optional single OAuth scope string required to read this field
    * `:requires_scopes` — optional list of OAuth scope strings (any one satisfies)
  """

  @enforce_keys [:name, :type]
  defstruct [
    :name,
    :type,
    nullable: false,
    description: nil,
    requires_scope: nil,
    requires_scopes: nil
  ]

  @type t :: %__MODULE__{
          name: String.t(),
          type: String.t(),
          nullable: boolean(),
          description: String.t() | nil,
          requires_scope: String.t() | nil,
          requires_scopes: [String.t()] | nil
        }
end

defmodule FraiseQL.ArgumentDefinition do
  @moduledoc """
  Represents an argument on a FraiseQL query or mutation.

  ## Fields

    * `:name` — the argument name as it appears in GraphQL
    * `:type` — the GraphQL type string, e.g. `"ID"`, `"String"`
    * `:nullable` — whether the argument is optional; defaults to `false`
    * `:description` — optional human-readable description
  """

  @enforce_keys [:name, :type]
  defstruct [:name, :type, nullable: false, description: nil]

  @type t :: %__MODULE__{
          name: String.t(),
          type: String.t(),
          nullable: boolean(),
          description: String.t() | nil
        }
end

defmodule FraiseQL.TypeDefinition do
  @moduledoc """
  Represents a FraiseQL object type backed by a SQL view.

  ## Fields

    * `:name` — the GraphQL type name, e.g. `"Author"`
    * `:sql_source` — the underlying view or table name, e.g. `"v_author"`
    * `:description` — optional human-readable description
    * `:fields` — list of `FraiseQL.FieldDefinition` structs
    * `:is_input` — whether this is a GraphQL input type; defaults to `false`
    * `:relay` — whether this type participates in Relay pagination; defaults to `false`
    * `:is_error` — whether this type represents a mutation error shape; defaults to `false`
    * `:tenant_scoped` — whether this type is scoped to a tenant; defaults to `false`
    * `:crud` — auto-generate CRUD operations; `false`, `true`, or list of strings like
      `["read", "create", "update", "delete"]`; defaults to `false`
    * `:key_fields` — list of federation key field names (default `nil`; defaults to `["id"]` at export)
    * `:extends_type` — boolean, marks this type as extending a type from another subgraph (default `false`)
  """

  @enforce_keys [:name, :sql_source]
  defstruct [
    :name,
    :sql_source,
    description: nil,
    fields: [],
    is_input: false,
    relay: false,
    is_error: false,
    tenant_scoped: false,
    crud: false,
    key_fields: nil,
    extends_type: false
  ]

  @type t :: %__MODULE__{
          name: String.t(),
          sql_source: String.t(),
          description: String.t() | nil,
          fields: [FraiseQL.FieldDefinition.t()],
          is_input: boolean(),
          relay: boolean(),
          is_error: boolean(),
          tenant_scoped: boolean(),
          crud: boolean() | [String.t()],
          key_fields: [String.t()] | nil,
          extends_type: boolean()
        }
end

defmodule FraiseQL.QueryDefinition do
  @moduledoc """
  Represents a FraiseQL query backed by a SQL view.

  ## Fields

    * `:name` — the GraphQL query field name, e.g. `"authors"`
    * `:return_type` — the GraphQL return type name, e.g. `"Author"`
    * `:sql_source` — the underlying view or table name
    * `:returns_list` — whether the query returns a list; defaults to `false`
    * `:nullable` — whether the query result can be null; defaults to `false`
    * `:arguments` — list of `FraiseQL.ArgumentDefinition` structs
    * `:cache_ttl_seconds` — optional cache TTL in seconds
    * `:description` — optional human-readable description
    * `:inject_params` — optional list of `%{"name" => ..., "source" => ..., "path" => ...}` maps
  """

  @enforce_keys [:name, :return_type, :sql_source]
  defstruct [
    :name,
    :return_type,
    :sql_source,
    returns_list: false,
    nullable: false,
    arguments: [],
    cache_ttl_seconds: nil,
    description: nil,
    rest_path: nil,
    rest_method: nil,
    inject_params: nil
  ]

  @type t :: %__MODULE__{
          name: String.t(),
          return_type: String.t(),
          sql_source: String.t(),
          returns_list: boolean(),
          nullable: boolean(),
          arguments: [FraiseQL.ArgumentDefinition.t()],
          cache_ttl_seconds: non_neg_integer() | nil,
          description: String.t() | nil,
          rest_path: String.t() | nil,
          rest_method: String.t() | nil,
          inject_params: [map()] | nil
        }
end

defmodule FraiseQL.MutationDefinition do
  @moduledoc """
  Represents a FraiseQL mutation backed by a SQL function.

  ## Fields

    * `:name` — the GraphQL mutation field name in camelCase, e.g. `"createAuthor"`
    * `:return_type` — the GraphQL return type name
    * `:sql_source` — the underlying function name, e.g. `"fn_create_author"`
    * `:operation` — the mutation operation type: `"insert"`, `"update"`, or `"delete"`
    * `:arguments` — list of `FraiseQL.ArgumentDefinition` structs
    * `:description` — optional human-readable description
    * `:inject_params` — optional list of `%{"name" => ..., "source" => ..., "path" => ...}` maps
  """

  @enforce_keys [:name, :return_type, :sql_source, :operation]
  defstruct [:name, :return_type, :sql_source, :operation, arguments: [], description: nil, rest_path: nil, rest_method: nil, inject_params: nil]

  @type t :: %__MODULE__{
          name: String.t(),
          return_type: String.t(),
          sql_source: String.t(),
          operation: String.t(),
          arguments: [FraiseQL.ArgumentDefinition.t()],
          description: String.t() | nil,
          rest_path: String.t() | nil,
          rest_method: String.t() | nil,
          inject_params: [map()] | nil
        }
end

defmodule FraiseQL.IntermediateSchema do
  @moduledoc """
  The top-level intermediate schema structure produced by `FraiseQL.SchemaExporter`.

  This is serialised to `schema.json` and consumed by `fraiseql compile`.

  ## Fields

    * `:version` — schema format version; defaults to `"2.0.0"`
    * `:types` — list of `FraiseQL.TypeDefinition` structs
    * `:queries` — list of `FraiseQL.QueryDefinition` structs
    * `:mutations` — list of `FraiseQL.MutationDefinition` structs
  """

  defstruct version: "2.0.0", types: [], queries: [], mutations: []

  @type t :: %__MODULE__{
          version: String.t(),
          types: [FraiseQL.TypeDefinition.t()],
          queries: [FraiseQL.QueryDefinition.t()],
          mutations: [FraiseQL.MutationDefinition.t()]
        }
end
