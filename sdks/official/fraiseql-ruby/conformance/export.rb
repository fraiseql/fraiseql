# frozen_string_literal: true

# Authors the cross-SDK conformance fixture with the Ruby SDK's public API.
#
# Driven by `sdks/official/conformance/run.py`; see
# `sdks/official/conformance/README.md`.
#
# The one rule for every SDK's copy of this file: author through the SDK, never
# hand-assemble the JSON. The pre-existing `test/generate_parity_schema.rb` built the
# expected document as a literal Hash and never loaded the gem at all, so it passed
# whatever the SDK did — which is how the SDK came to have no schema exporter, a README
# documenting one, and a `to_fraiseql_schema` that omitted the required `nullable` key
# (#853, #854).

$LOAD_PATH.unshift(File.expand_path("../lib", __dir__))

require "fraiseql"

def author_minimal
  schema = FraiseQL::Schema.new

  schema.type "User", sql_source: "v_user" do |t|
    t.field :id, :id, nullable: false
    t.field :email, :string, nullable: false
  end

  schema.query "users", return_type: "User", returns_list: true, nullable: false,
                        sql_source: "v_user"
  schema
end

def author_full
  schema = FraiseQL::Schema.new

  schema.type "User", sql_source: "v_user", relay: true do |t|
    t.field :id, :id, nullable: false
    t.field :email, :string, nullable: false
    t.field :name, :string, nullable: true, description: 'The user\'s "display" name',
                           deprecated: "use displayName"
    t.field :salary, :float, nullable: true, requires_scope: "read:User.salary"
  end

  schema.type "Order", sql_source: "v_order" do |t|
    t.field :id, :id, nullable: false
    t.field :total, :float, nullable: false
    t.field :status, :string, nullable: false
  end

  schema.type "UserNotFound", sql_source: "v_user_not_found", is_error: true do |t|
    t.field :message, :string, nullable: false
    t.field :code, :string, nullable: false
  end

  schema.type "Document", sql_source: "v_document" do |t|
    t.field :id, :id, nullable: false
    t.field :embedding, :vector, nullable: false,
                                 vector_config: { dimensions: 1536, index_type: :ivf_flat,
                                                  distance_metric: :l2 }
    t.field :fingerprint, :bit_vector, nullable: false,
                                       vector_config: { dimensions: 768, distance_metric: :hamming }
    t.field :compact, :half_vector, nullable: true,
                                    vector_config: { dimensions: 1536,
                                                     distance_metric: :inner_product }
    t.field :terms, :sparse_vector, nullable: true,
                                    vector_config: { dimensions: 30_000, index_type: :none }
    t.field :similarity, :float, nullable: false, vector_distance: :embedding
  end

  schema.type "CreateUserInput", is_input: true do |t|
    t.field :email, :string, nullable: false
    t.field :name, :string, nullable: true
  end

  schema.enum "OrderStatus", %w[PENDING SHIPPED CANCELLED]

  schema.query "users", return_type: "User", returns_list: true, nullable: false,
                        sql_source: "v_user"

  schema.query "user", return_type: "User", returns_list: false, nullable: true,
                       sql_source: "v_user" do |q|
    q.argument :id, :id, nullable: false
  end

  schema.query "tenantOrders", return_type: "Order", returns_list: true, nullable: false,
                               sql_source: "v_order",
                               inject: { "tenant_id" => "jwt:tenant_id" },
                               cache_ttl_seconds: 300,
                               requires_role: "admin"

  schema.mutation "createUser", return_type: "User", sql_source: "fn_create_user",
                                operation: "insert",
                                invalidates_views: %w[v_user v_user_summary],
                                invalidates_fact_tables: %w[tf_signup] do |m|
    m.argument :email, :string, nullable: false
    m.argument :name, :string, nullable: true
  end

  schema.mutation "placeOrder", return_type: "Order", sql_source: "fn_place_order",
                                operation: "insert",
                                inject: { "user_id" => "jwt:sub" },
                                invalidates_views: %w[v_order_summary],
                                invalidates_fact_tables: %w[tf_sale]

  schema
end

fixture = ENV.fetch("FRAISEQL_CONFORMANCE_FIXTURE", nil)
out = ENV.fetch("FRAISEQL_CONFORMANCE_OUT", nil)

if fixture.nil? || out.nil?
  warn "FRAISEQL_CONFORMANCE_FIXTURE and FRAISEQL_CONFORMANCE_OUT must be set"
  exit 2
end

schema =
  case fixture
  when "minimal" then author_minimal
  when "full" then author_full
  else raise ArgumentError, "unknown fixture #{fixture}"
  end

schema.export_json(out)
