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

  schema.query :users, return_type: "User", returns_list: true, nullable: false,
                       sql_source: "v_user"
  schema
end

def author_full
  schema = FraiseQL::Schema.new

  # Both directions, deliberately (#1266): which join column is read off which side swaps
  # with the cardinality, so a fixture carrying only `OneToMany` would be uniform in
  # exactly the dimension that selects the branch. The keys name SQL **columns**
  # (`fk_user`) while `Order` publishes the field `:fk_user` as `fkUser`.
  schema.type "User", sql_source: "v_user", relay: true, relationships: [
    { name: "orders", target_type: "Order", cardinality: "OneToMany",
      foreign_key: "fk_user", referenced_key: "id" }
  ] do |t|
    t.field :id, :id, nullable: false
    t.field :email, :string, nullable: false
    t.field :name, :string, nullable: true, description: 'The user\'s "display" name',
                           deprecated: "use displayName"
    t.field :salary, :float, nullable: true, requires_scope: "read:User.salary"
    # Two words and a digit segment (#1249). A Ruby field name is a snake_case symbol,
    # so this is one of the fixtures that actually discriminates: `:last_login_at` must
    # reach the wire as `lastLoginAt`, and `:phone_1` as `phone1` — the reference
    # collapses a digit segment onto the previous word, which a `/_([a-z])/` helper
    # silently does not.
    t.field :last_login_at, :string, nullable: true
    t.field :phone_1, :string, nullable: true
  end

  schema.type "Order", sql_source: "v_order", relationships: [
    { name: "user", target_type: "User", cardinality: "ManyToOne",
      foreign_key: "fk_user", referenced_key: "id" }
  ] do |t|
    t.field :id, :id, nullable: false
    t.field :total, :float, nullable: false
    t.field :status, :string, nullable: false
    # The column `User.orders` joins on, published under the naming convention.
    t.field :fk_user, :id, nullable: false
  end

  # `crud` is an authoring-time expansion the compiler has no concept of, so the only
  # evidence this SDK implements it is that the operations and input objects appear in the
  # compiled schema. `computed` is the same: emitting the flag makes the document
  # uncompilable, so the sole evidence it was honoured is `slug` on the type and
  # absent from both input objects.
  schema.type "SupportTicket", sql_source: "v_support_ticket", crud: true do |t|
    t.field :id, :int, nullable: false
    t.field :title, :string, nullable: false
    t.field :due_date, :string, nullable: false
    t.field :slug, :string, nullable: false, computed: true
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
    # Two words: a hand-authored input type's field names are a third registration path,
    # distinct from a type's fields and from a `crud` type's generated input objects
    # (#1249 covered those two), and no fixture name reached it (#1255).
    t.field :display_name, :string, nullable: true
  end

  schema.enum "OrderStatus", %w[PENDING SHIPPED CANCELLED]

  schema.query :users, return_type: "User", returns_list: true, nullable: false,
                       sql_source: "v_user"

  # Operation and argument names are authored in Ruby's idiom — a snake_case symbol — and
  # must reach the API camelCased, exactly as `t.field :due_date` does (#1249). Every
  # operation name in this fixture used to be a camelCase *string*, which is not how a Ruby
  # author writes one, so `schema.rb`'s `name.to_s` ran on every export and never once saw
  # an input the two conventions spell differently (#1255).
  schema.query :user, return_type: "User", returns_list: false, nullable: true,
                      sql_source: "v_user" do |q|
    q.argument :id, :id, nullable: false
  end

  schema.query :tenant_orders, return_type: "Order", returns_list: true, nullable: false,
                               sql_source: "v_order",
                               inject: { "tenant_id" => "jwt:tenant_id" },
                               cache_ttl_seconds: 300,
                               requires_role: "admin",
                               # #966's actor allow-list, enforced in the same executor
                               # gate as requires_role on every transport, and authorable
                               # in no SDK until #1123.
                               requires_actor: %w[human_user service_account] do |q|
    q.argument :include_archived, :boolean, nullable: true
  end

  schema.mutation :create_user, return_type: "User", sql_source: "fn_create_user",
                                operation: "insert",
                                invalidates_views: %w[v_user v_user_summary],
                                invalidates_fact_tables: %w[tf_signup],
                                # #1253: the role gate on the write side, implemented in
                                # all eleven mutation builders and compared in none until
                                # this construct.
                                requires_role: "admin",
                                requires_actor: %w[service_account] do |m|
    m.argument :email, :string, nullable: false
    m.argument :name, :string, nullable: true
    m.argument :display_name, :string, nullable: true
  end

  schema.mutation :place_order, return_type: "Order", sql_source: "fn_place_order",
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
