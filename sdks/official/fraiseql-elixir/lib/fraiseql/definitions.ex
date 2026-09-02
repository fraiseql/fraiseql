defmodule FraiseQL.VectorConfig do
  @moduledoc """
  pgvector configuration for a vector field.

  The compiler refuses a `Vector`, `BitVector`, `HalfVector` or `SparseVector` field that
  carries no configuration, so this is what makes those types authorable.

  Which combinations of field type, metric and index exist is pgvector's business and the
  compiler's: it holds the operator-class table — `ivfflat` has no class for a sparse
  vector at all, and none for jaccard — and refuses a schema that asks for one that does
  not, naming the alternative. This SDK carries no second copy of that table; a copy is
  what drifts.

  ## Fields

    * `:dimensions` — vector width: float components for `Vector`, `HalfVector` and
      `SparseVector`, **bits** for `BitVector`. It sizes the column, and a query vector of
      a different width is refused rather than silently padded.
    * `:index_type` — `"hnsw"` (default), `"ivf_flat"` or `"none"` for exact search
    * `:distance_metric` — `"cosine"` (default), `"l2"` or `"inner_product"` for float
      vectors; `"hamming"` or `"jaccard"` for a `BitVector`

  ## Example

      field :embedding, :vector,
        nullable: false,
        vector_config: [dimensions: 1536, index_type: :ivf_flat, distance_metric: :l2]
  """

  @enforce_keys [:dimensions]
  defstruct dimensions: nil, index_type: "hnsw", distance_metric: "cosine"

  @type t :: %__MODULE__{
          dimensions: pos_integer(),
          index_type: String.t(),
          distance_metric: String.t()
        }

  @doc """
  Build a config from a keyword list, filling the two defaults.

  The index type and the metric are written out even where the author left them off, so
  the emitted schema says which index and which metric the column will get rather than
  leaving it to a compiler default the author cannot see. Atoms and strings are both
  accepted — an Elixir author reaches for `:ivf_flat`, the wire format wants
  `"ivf_flat"`.
  """
  @spec new(keyword() | t()) :: t()
  def new(%__MODULE__{} = config), do: config

  def new(opts) when is_list(opts) do
    dimensions = Keyword.get(opts, :dimensions)

    unless is_integer(dimensions) and dimensions >= 1 do
      raise ArgumentError,
            "vector_config requires dimensions of at least 1, got #{inspect(dimensions)}"
    end

    %__MODULE__{
      dimensions: dimensions,
      index_type: to_string(Keyword.get(opts, :index_type, "hnsw")),
      distance_metric: to_string(Keyword.get(opts, :distance_metric, "cosine"))
    }
  end
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
    * `:computed` — when `true`, this field is server-computed and excluded from CRUD
      input types (`CreateXInput`, `UpdateXInput`); defaults to `false`
    * `:vector_config` — a `FraiseQL.VectorConfig` on a `Vector` / `BitVector` /
      `HalfVector` / `SparseVector` field. The compiler refuses such a field without one.
    * `:vector_distance` — on a `Float` field, the vector field whose `nearest` search
      distance this field carries. Selecting it on a query that did not run that search
      is refused, not answered with null.
    * `:deprecated` — `true` for deprecated with no stated reason, or the reason as a
      string. Surfaces as `isDeprecated` / `deprecationReason` through introspection.
  """

  @enforce_keys [:name, :type]
  defstruct [
    :name,
    :type,
    nullable: false,
    description: nil,
    requires_scope: nil,
    requires_scopes: nil,
    computed: false,
    vector_config: nil,
    vector_distance: nil,
    deprecated: nil
  ]

  @type t :: %__MODULE__{
          name: String.t(),
          type: String.t(),
          nullable: boolean(),
          description: String.t() | nil,
          requires_scope: String.t() | nil,
          requires_scopes: [String.t()] | nil,
          computed: boolean(),
          vector_config: FraiseQL.VectorConfig.t() | nil,
          vector_distance: String.t() | nil,
          deprecated: boolean() | String.t() | nil
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
    * `:relationships` — list of `FraiseQL.Relationship` structs followed by REST resource
      embedding (#1266); defaults to `[]`
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
    crud: false,
    cascade: false,
    relationships: []
  ]

  @type t :: %__MODULE__{
          name: String.t(),
          sql_source: String.t(),
          description: String.t() | nil,
          fields: [FraiseQL.FieldDefinition.t()],
          is_input: boolean(),
          relay: boolean(),
          is_error: boolean(),
          crud: boolean() | [atom()],
          cascade: boolean(),
          relationships: [FraiseQL.Relationship.t()]
        }
end

defmodule FraiseQL.Relationship do
  @moduledoc """
  A relationship to another type, followed by REST resource embedding (#1266).

  `:name` is what a client writes in `?select=orders(id,total)`, `?select=orders.count`
  and `?orders.status=paid`; it is also what the generated client's `relationships`
  module and the served OpenAPI document publish.

  `:foreign_key` and `:referenced_key` are SQL **column** names, and which side each is
  read from swaps with the cardinality — `"OneToMany"` reads `:referenced_key` off the
  declaring type and filters `:foreign_key` on the target; `"ManyToOne"` and `"OneToOne"`
  do the reverse. Under the default `camelCase` naming convention the column `fk_user` is
  published as the field `fkUser`, and the compiler resolves one to the other.

  Which relationships are *followable* is the compiler's business, not this SDK's: it
  refuses a target type it does not declare, a join column no field on that side
  publishes, and a target no list query returns. This SDK carries no second copy of those
  rules; a copy is what drifts.

  ## Fields

    * `:name` — the relationship name
    * `:target_type` — target type name; must be returned by some **list** query
    * `:cardinality` — `"OneToMany"`, `"ManyToOne"` or `"OneToOne"`
    * `:foreign_key` — foreign key column on the child table
    * `:referenced_key` — referenced key column on the parent table
  """

  @cardinalities ["OneToMany", "ManyToOne", "OneToOne"]

  @enforce_keys [:name, :target_type, :cardinality, :foreign_key, :referenced_key]
  defstruct [:name, :target_type, :cardinality, :foreign_key, :referenced_key]

  @type t :: %__MODULE__{
          name: String.t(),
          target_type: String.t(),
          cardinality: String.t(),
          foreign_key: String.t(),
          referenced_key: String.t()
        }

  @doc "The cardinalities a relationship may declare."
  def cardinalities, do: @cardinalities

  @doc """
  Build a relationship from a keyword list, raising on a shape this SDK can reject.

  Only the shape this SDK owns is checked — a blank key, an unknown cardinality. Whether
  the relationship can be *followed* is checked by the compiler against the whole schema,
  which is the only place that knows.
  """
  def new!(opts) do
    values =
      for key <- [:name, :target_type, :cardinality, :foreign_key, :referenced_key], into: %{} do
        value = Keyword.get(opts, key)

        if !is_binary(value) or value == "" do
          raise ArgumentError,
                "relationship #{key} must be a non-empty string, got: #{inspect(value)}"
        end

        {key, value}
      end

    if values.cardinality not in @cardinalities do
      raise ArgumentError,
            "relationship cardinality must be one of #{inspect(@cardinalities)}, " <>
              "got: #{inspect(values.cardinality)}"
    end

    struct!(__MODULE__, Map.to_list(values))
  end
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
    * `:auto_params` — optional map of auto-generated parameter flags (e.g. `%{where: true, order_by: true}`)
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
    auto_params: nil,
    inject_params: nil,
    requires_role: nil,
    requires_actor: []
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
          auto_params: map() | nil,
          inject_params: map() | nil,
          requires_role: String.t() | nil,
          requires_actor: [String.t()]
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
    * `:cascade` — whether this mutation uses GraphQL cascade; defaults to `false`
  """

  @enforce_keys [:name, :return_type, :sql_source, :operation]
  defstruct [
    :name,
    :return_type,
    :sql_source,
    :operation,
    arguments: [],
    description: nil,
    rest_path: nil,
    rest_method: nil,
    cascade: false,
    inject_params: nil,
    requires_role: nil,
    requires_actor: [],
    invalidates_views: nil,
    invalidates_fact_tables: nil
  ]

  @type t :: %__MODULE__{
          name: String.t(),
          return_type: String.t(),
          sql_source: String.t(),
          operation: String.t(),
          arguments: [FraiseQL.ArgumentDefinition.t()],
          description: String.t() | nil,
          rest_path: String.t() | nil,
          rest_method: String.t() | nil,
          cascade: boolean(),
          inject_params: map() | nil,
          requires_role: String.t() | nil,
          requires_actor: [String.t()],
          invalidates_views: [String.t()] | nil,
          invalidates_fact_tables: [String.t()] | nil
        }
end

defmodule FraiseQL.EnumDefinition do
  @moduledoc """
  Represents a FraiseQL GraphQL enum type.

  ## Fields

    * `:name` — the GraphQL enum type name, e.g. `"OrderStatus"`
    * `:values` — the member names, in declaration order
    * `:description` — optional human-readable description
  """

  @enforce_keys [:name, :values]
  defstruct [:name, :values, description: nil]

  @type t :: %__MODULE__{
          name: String.t(),
          values: [String.t()],
          description: String.t() | nil
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

  defstruct version: "2.0.0", types: [], queries: [], mutations: [], enums: []

  @type t :: %__MODULE__{
          version: String.t(),
          types: [FraiseQL.TypeDefinition.t()],
          enums: [FraiseQL.EnumDefinition.t()],
          queries: [FraiseQL.QueryDefinition.t()],
          mutations: [FraiseQL.MutationDefinition.t()]
        }
end
