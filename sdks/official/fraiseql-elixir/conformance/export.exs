# Authors the cross-SDK conformance fixture with the Elixir SDK's public API.
#
# Driven by `sdks/official/conformance/run.py`; see
# `sdks/official/conformance/README.md`.
#
# The one rule for every SDK's copy of this file: author through the SDK, never
# hand-assemble the JSON.
#
# Note the naming idiom: `fraiseql_type` takes a string, while `fraiseql_query` and
# `fraiseql_mutation` take atoms — the query name is the atom verbatim and the mutation
# name is its camelCase form, so `:place_order` declares `placeOrder`. The pre-existing `generate_parity_schema_test.exs` built the
# expected document as a literal map and never touched `FraiseQL.SchemaExporter`, so it
# was structurally incapable of failing.

defmodule Conformance.MinimalSchema do
  use FraiseQL.Schema

  fraiseql_type "User", sql_source: "v_user" do
    field(:id, :id, nullable: false)
    field(:email, :string, nullable: false)
  end

  fraiseql_query(:users,
    return_type: "User",
    returns_list: true,
    nullable: false,
    sql_source: "v_user"
  )
end

defmodule Conformance.FullSchema do
  use FraiseQL.Schema

  fraiseql_type "User", sql_source: "v_user", relay: true do
    field(:id, :id, nullable: false)
    field(:email, :string, nullable: false)
    field(:name, :string,
      nullable: true,
      description: ~s(The user's "display" name),
      deprecated: "use displayName"
    )
    field(:salary, :float, nullable: true, requires_scope: "read:User.salary")
  end

  fraiseql_type "Order", sql_source: "v_order" do
    field(:id, :id, nullable: false)
    field(:total, :float, nullable: false)
    field(:status, :string, nullable: false)
  end

  # `crud` is an authoring-time expansion the compiler has no concept of, so the only
  # evidence this SDK implements it is that the operations and input objects appear in the
  # compiled schema. `computed` is the same: emitting the flag makes the document
  # uncompilable, so the sole evidence it was honoured is `slug` on the type and absent
  # from both input objects.
  fraiseql_type "SupportTicket", sql_source: "v_support_ticket", crud: true do
    field :id, :int, nullable: false
    field :title, :string, nullable: false
    field :slug, :string, nullable: false, computed: true
  end

  fraiseql_type "UserNotFound", sql_source: "v_user_not_found", is_error: true do
    field(:message, :string, nullable: false)
    field(:code, :string, nullable: false)
  end

  fraiseql_type "Document", sql_source: "v_document" do
    field(:id, :id, nullable: false)

    field(:embedding, :vector,
      nullable: false,
      vector_config: [dimensions: 1536, index_type: :ivf_flat, distance_metric: :l2]
    )

    field(:fingerprint, :bit_vector,
      nullable: false,
      vector_config: [dimensions: 768, distance_metric: :hamming]
    )

    field(:compact, :half_vector,
      nullable: true,
      vector_config: [dimensions: 1536, distance_metric: :inner_product]
    )

    field(:terms, :sparse_vector,
      nullable: true,
      vector_config: [dimensions: 30000, index_type: :none]
    )

    field(:similarity, :float, nullable: false, vector_distance: :embedding)
  end

  # `is_input: true` is this SDK's only route to an input object — its exporter emits no
  # `input_types` key, and the compiler reclassifies a type carrying the flag.
  fraiseql_type "CreateUserInput", is_input: true do
    field(:email, :string, nullable: false)
    field(:name, :string, nullable: true)
  end

  fraiseql_enum("OrderStatus", values: ["PENDING", "SHIPPED", "CANCELLED"])

  fraiseql_query(:users,
    return_type: "User",
    returns_list: true,
    nullable: false,
    sql_source: "v_user"
  )

  fraiseql_query :user,
                 return_type: "User",
                 returns_list: false,
                 nullable: true,
                 sql_source: "v_user" do
    argument(:id, :id, nullable: false)
  end

  fraiseql_query(:tenantOrders,
    return_type: "Order",
    returns_list: true,
    nullable: false,
    sql_source: "v_order",
    inject_params: %{"tenant_id" => "jwt:tenant_id"},
    cache_ttl_seconds: 300,
    requires_role: "admin",
    # #966's actor allow-list, enforced in the same executor gate as requires_role on every
    # transport, and authorable in no SDK until #1123.
    requires_actor: ["human_user", "service_account"]
  )

  fraiseql_mutation :create_user,
                    return_type: "User",
                    sql_source: "fn_create_user",
                    operation: "insert",
                    invalidates_views: ["v_user", "v_user_summary"],
                    invalidates_fact_tables: ["tf_signup"],
                    requires_actor: ["service_account"] do
    argument(:email, :string, nullable: false)
    argument(:name, :string, nullable: true)
  end

  fraiseql_mutation(:place_order,
    return_type: "Order",
    sql_source: "fn_place_order",
    operation: "insert",
    inject_params: %{"user_id" => "jwt:sub"},
    invalidates_views: ["v_order_summary"],
    invalidates_fact_tables: ["tf_sale"]
  )
end

fixture = System.get_env("FRAISEQL_CONFORMANCE_FIXTURE")
out = System.get_env("FRAISEQL_CONFORMANCE_OUT")

if is_nil(fixture) or is_nil(out) do
  IO.puts(:stderr, "FRAISEQL_CONFORMANCE_FIXTURE and FRAISEQL_CONFORMANCE_OUT must be set")
  System.halt(2)
end

module =
  case fixture do
    "minimal" -> Conformance.MinimalSchema
    "full" -> Conformance.FullSchema
    other -> raise ArgumentError, "unknown fixture #{other}"
  end

:ok = FraiseQL.SchemaExporter.export_to_file!(module, out)
