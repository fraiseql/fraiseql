# frozen_string_literal: true

require_relative "test_helper"

# ── Type definition tests ─────────────────────────────────────────────────

class Product
  include FraiseQL::Type

  fraiseql_field :id,    :ID,     required: true
  fraiseql_field :name,  :String, required: true, description: "Product name"
  fraiseql_field :price, :Float,  required: true, deprecated: true
end

class Widget
  include FraiseQL::Type

  fraiseql_field :legacy, :String, required: false, deprecated: "use modern"
end

class TypeDefinitionTest < Minitest::Test
  def test_type_name_defaults_to_class_name
    assert_equal "Product", Product.fraiseql_type_name
  end

  def test_sql_source_defaults_to_snake_case_view
    assert_equal "v_product", Product.fraiseql_sql_source
  end

  def test_schema_contains_fields
    schema = Product.to_fraiseql_schema
    assert_equal "Product", schema[:name]
    assert_equal 3, schema[:fields].length
    names = schema[:fields].map { |f| f[:name] }
    assert_includes names, "id"
    assert_includes names, "name"
    assert_includes names, "price"
  end

  def test_field_description_included_when_present
    schema = Product.to_fraiseql_schema
    name_field = schema[:fields].find { |f| f[:name] == "name" }
    assert_equal "Product name", name_field[:description]
  end

  # Inverted, not deleted. `IntermediateField` had no `deprecated` member until #1025,
  # so dropping the key was right at the time and this test was pinning it. Now the
  # compiler reads it and the reason reaches introspection, so dropping it is a silent
  # loss of what the author declared — the exact shape the conformance suite's
  # `field_deprecated` construct exists to catch.
  #
  # `true` is deprecated with no stated reason, which the compiler models as an absent
  # `reason`.
  def test_deprecated_field_is_emitted_in_the_compilers_shape
    schema = Product.to_fraiseql_schema
    price_field = schema[:fields].find { |f| f[:name] == "price" }
    assert_equal({}, price_field[:deprecated])
  end

  def test_deprecation_reason_is_carried
    field = Widget.to_fraiseql_schema[:fields].first
    assert_equal({ reason: "use modern" }, field[:deprecated])
  end

  def test_undeprecated_field_drops_the_key
    schema = Product.to_fraiseql_schema
    name_field = schema[:fields].find { |f| f[:name] == "name" }
    refute name_field.key?(:deprecated)
  end

  def test_crud_disabled_by_default
    refute Product.fraiseql_crud_enabled?
    assert_nil Product.to_fraiseql_crud
  end
end

# ── Custom type name + CRUD ───────────────────────────────────────────────

class OrderItem
  include FraiseQL::Type

  fraiseql_type_name "LineItem"
  fraiseql_sql_source "v_line_item"
  fraiseql_crud true
  fraiseql_cascade true

  fraiseql_field :id,       :ID,     required: true
  fraiseql_field :quantity, :Int,    required: true
  fraiseql_field :total,    :Float,  required: true
  # Server-assigned, so a client cannot supply it: the generated input objects must omit
  # it. Nothing here declared a computed field before, which is why the generator's
  # `reject { |f| f[:computed] }` could not be seen to reject nothing (#1242).
  fraiseql_field :line_total, :Float, required: true, computed: true
end

class CustomTypeTest < Minitest::Test
  def test_custom_type_name
    assert_equal "LineItem", OrderItem.fraiseql_type_name
  end

  def test_custom_sql_source
    assert_equal "v_line_item", OrderItem.fraiseql_sql_source
  end

  def test_crud_enabled
    assert OrderItem.fraiseql_crud_enabled?
  end

  def test_cascade_enabled
    assert OrderItem.fraiseql_cascade_enabled?
  end

  def test_crud_generates_queries_and_mutations
    crud = OrderItem.to_fraiseql_crud
    refute_nil crud
    assert crud[:queries].length >= 2, "Expected at least 2 queries (get + list)"
    assert crud[:mutations].length >= 3, "Expected at least 3 mutations (create + update + delete)"
  end

  # These two assertions used to be `length >= 2` / `>= 3` and nothing else, so the whole
  # generated shape was free to be anything — which is how Ruby came to emit flat arguments
  # and no input types while six other SDKs emitted an input object (#1246).
  def test_crud_create_and_update_take_an_input_object
    crud = OrderItem.to_fraiseql_crud
    create = crud[:mutations].find { |m| m[:name] == "createLineItem" }
    update = crud[:mutations].find { |m| m[:name] == "updateLineItem" }

    assert_equal [{ name: "input", type: "CreateLineItemInput", nullable: false }], create[:arguments]
    assert_equal [{ name: "input", type: "UpdateLineItemInput", nullable: false }], update[:arguments]
    assert_equal %w[CreateLineItemInput UpdateLineItemInput],
                 crud[:input_types].map { |i| i[:name] }.sort
  end

  def test_crud_input_objects_omit_computed_fields
    crud = OrderItem.to_fraiseql_crud
    create = crud[:input_types].find { |i| i[:name] == "CreateLineItemInput" }

    refute_includes create[:fields].map { |f| f[:name] }, "lineTotal",
                    "a computed field is server-assigned; a client cannot supply one"
  end

  def test_crud_mutations_include_cascade
    crud = OrderItem.to_fraiseql_crud
    crud[:mutations].each do |m|
      assert_equal true, m[:cascade], "Mutation #{m[:name]} should have cascade: true"
    end
  end
end

# ── CRUD generator unit tests ─────────────────────────────────────────────

class CrudGeneratorTest < Minitest::Test
  def test_pascal_to_snake
    assert_equal "order_item", FraiseQL::CrudGenerator.pascal_to_snake("OrderItem")
    assert_equal "user", FraiseQL::CrudGenerator.pascal_to_snake("User")
  end

  def test_snake_to_camel
    assert_equal "orderItem", FraiseQL::CrudGenerator.snake_to_camel("order_item")
    assert_equal "id", FraiseQL::CrudGenerator.snake_to_camel("id")
  end

  # The two properties both one-word cases above are blind to. The helper was
  # `/_([a-z])/`, whose character class does not match a digit, so `phone_1` kept its
  # underscore while the engine and every other SDK produced `phone1`; and it left the
  # rest of a segment alone, which is right — `user_ID` is `userID`, not `userId`
  # (#1249). The reference is `fraiseql-core/src/utils/casing.rs`.
  def test_snake_to_camel_collapses_a_digit_segment
    assert_equal "phone1", FraiseQL::Naming.snake_to_camel("phone_1")
    assert_equal "dns1Id", FraiseQL::Naming.snake_to_camel("dns_1_id")
  end

  def test_snake_to_camel_keeps_the_rest_of_a_segment
    assert_equal "userID", FraiseQL::Naming.snake_to_camel("user_ID")
  end

  def test_snake_to_camel_is_idempotent_and_accepts_a_symbol
    assert_equal "lastLoginAt", FraiseQL::Naming.snake_to_camel(:last_login_at)
    assert_equal "lastLoginAt", FraiseQL::Naming.snake_to_camel("lastLoginAt")
  end

  # One gem, one answer. `CrudGenerator` camelCased what it generated while the type
  # builder emitted the symbol verbatim, so the same column was `due_date` on the type
  # and `dueDate` in `CreateXInput` (#1249).
  def test_type_builder_and_crud_generator_agree_on_a_two_word_field
    schema = FraiseQL::Schema.new
    schema.type "SupportTicket", sql_source: "v_support_ticket", crud: true do |t|
      t.field :id, :int, nullable: false
      t.field :due_date, :string, nullable: false
    end
    doc = schema.to_h

    # Input objects are appended to `types` with `is_input`, which is the spelling
    # `Schema#expand_crud` uses and the conformance fixture proves the compiler accepts.
    ticket = doc["types"].find { |ty| ty["name"] == "SupportTicket" }
    create = doc["types"].find { |ty| ty["name"] == "CreateSupportTicketInput" }

    assert_equal %w[id dueDate], ticket["fields"].map { |f| f["name"] }
    assert_equal %w[id dueDate], create["fields"].map { |f| f["name"] }
  end

  def test_pluralize
    assert_equal "users", FraiseQL::CrudGenerator.pluralize("user")
    assert_equal "addresses", FraiseQL::CrudGenerator.pluralize("address")
    assert_equal "categories", FraiseQL::CrudGenerator.pluralize("category")
    assert_equal "items", FraiseQL::CrudGenerator.pluralize("items") # already plural
  end

  def test_generate_raises_on_empty_fields
    assert_raises(ArgumentError) do
      FraiseQL::CrudGenerator.generate(type_name: "Empty", fields: [])
    end
  end

  def test_generate_produces_correct_operation_names
    fields = [
      { name: "id", type: "ID", nullable: false },
      { name: "name", type: "String", nullable: false },
    ]
    result = FraiseQL::CrudGenerator.generate(type_name: "User", fields: fields)

    query_names = result[:queries].map { |q| q[:name] }
    assert_includes query_names, "user"    # get by ID
    assert_includes query_names, "users"   # list

    mutation_names = result[:mutations].map { |m| m[:name] }
    assert_includes mutation_names, "createUser"
    assert_includes mutation_names, "updateUser"
    assert_includes mutation_names, "deleteUser"
  end

  def test_generate_uses_custom_sql_source
    fields = [{ name: "id", type: "ID", nullable: false }]
    result = FraiseQL::CrudGenerator.generate(
      type_name: "Audit", fields: fields, sql_source: "v_audit_log"
    )
    get_query = result[:queries].find { |q| q[:name] == "audit" }
    assert_equal "v_audit_log", get_query[:sql_source]
  end
end

# ── Error classes ─────────────────────────────────────────────────────────

class ErrorTest < Minitest::Test
  def test_graphql_error_message_from_hash
    err = FraiseQL::GraphQLError.new([{ "message" => "Field not found" }])
    assert_equal "Field not found", err.message
    assert_equal 1, err.errors.length
  end

  def test_authentication_error_status_code
    err = FraiseQL::AuthenticationError.new(401)
    assert_equal 401, err.status_code
    assert_match(/401/, err.message)
  end

  def test_rate_limit_error_retry_after
    err = FraiseQL::RateLimitError.new(retry_after: 30)
    assert_equal 30, err.retry_after
  end

  def test_error_hierarchy
    assert_kind_of StandardError, FraiseQL::Error.new
    assert_kind_of FraiseQL::Error, FraiseQL::GraphQLError.new([{ "message" => "x" }])
    assert_kind_of FraiseQL::Error, FraiseQL::NetworkError.new
    assert_kind_of FraiseQL::NetworkError, FraiseQL::TimeoutError.new
  end
end

# ── Retry config ──────────────────────────────────────────────────────────

class RetryConfigTest < Minitest::Test
  def test_default_config
    config = FraiseQL::RetryConfig.new
    assert_equal 1, config.max_attempts
    assert_in_delta 1.0, config.base_delay
    assert_in_delta 30.0, config.max_delay
    assert config.jitter
  end

  def test_delay_increases_exponentially
    config = FraiseQL::RetryConfig.new(base_delay: 1.0, max_delay: 60.0, jitter: false)
    d0 = config.delay_for(0) # 1.0 * 2^0 = 1.0
    d1 = config.delay_for(1) # 1.0 * 2^1 = 2.0
    d2 = config.delay_for(2) # 1.0 * 2^2 = 4.0
    assert d1 > d0, "delay should increase: #{d1} > #{d0}"
    assert d2 > d1, "delay should increase: #{d2} > #{d1}"
  end

  def test_delay_capped_at_max
    config = FraiseQL::RetryConfig.new(base_delay: 1.0, max_delay: 5.0, jitter: false)
    d10 = config.delay_for(10) # 1.0 * 2^10 = 1024, capped at 5.0
    assert_in_delta 5.0, d10
  end

  def test_retryable_matches_configured_errors
    config = FraiseQL::RetryConfig.new(retry_on: [FraiseQL::NetworkError])
    assert config.retryable?(FraiseQL::NetworkError.new("fail"))
    assert config.retryable?(FraiseQL::TimeoutError.new("timeout")) # subclass
    refute config.retryable?(FraiseQL::GraphQLError.new([{ "message" => "x" }]))
  end
end

# ── Client construction ───────────────────────────────────────────────────

class ClientConstructionTest < Minitest::Test
  def test_client_accepts_url
    client = FraiseQL::Client.new("http://localhost:4000/graphql")
    refute_nil client
  end

  def test_client_with_authorization
    client = FraiseQL::Client.new(
      "http://localhost:4000",
      authorization: "Bearer test-token",
      timeout: 10
    )
    refute_nil client
  end

  def test_client_with_retry_config
    config = FraiseQL::RetryConfig.new(max_attempts: 3, base_delay: 0.5)
    client = FraiseQL::Client.new(
      "http://localhost:4000",
      retry_config: config
    )
    refute_nil client
  end
end

# ── Operation and argument names ──────────────────────────────────────────

# An operation or argument declared the way a Ruby author writes an identifier — a
# snake_case symbol — must reach the API camelCased, the same rule `t.field` has followed
# since #1249.
#
# `schema.rb` published these three verbatim through `name.to_s`, so `schema.query
# :tenant_orders` put `tenant_orders` in the GraphQL API where the identical declaration
# in Python put `tenantOrders`. Nothing compared them, because every operation name in the
# cross-SDK conformance fixture was written as a camelCase *string* — which is not how a
# Ruby author writes one — and every argument name in it was `id`, `email` or `name`,
# which spell the same in both conventions (#1255).
class OperationNamingTest < Minitest::Test
  def test_a_query_declared_as_a_snake_case_symbol_is_published_camelcase
    schema = FraiseQL::Schema.new
    schema.query :tenant_orders, return_type: "Order", returns_list: true, sql_source: "v_order"

    assert_equal "tenantOrders", schema.to_h["queries"].first["name"]
  end

  def test_a_mutation_declared_as_a_snake_case_symbol_is_published_camelcase
    schema = FraiseQL::Schema.new
    schema.mutation :create_user, return_type: "User", sql_source: "fn_create_user"

    assert_equal "createUser", schema.to_h["mutations"].first["name"]
  end

  def test_argument_names_are_published_camelcase_on_both_operation_kinds
    schema = FraiseQL::Schema.new
    schema.query(:tenant_orders, return_type: "Order", sql_source: "v_order") do |q|
      q.argument :include_archived, :boolean, nullable: true
    end
    schema.mutation(:create_user, return_type: "User", sql_source: "fn_create_user") do |m|
      m.argument :display_name, :string, nullable: true
    end

    hash = schema.to_h
    assert_equal "includeArchived", hash["queries"].first["arguments"].first["name"]
    assert_equal "displayName", hash["mutations"].first["arguments"].first["name"]
  end

  # A one-word name spells the same either way, so it can pass under either
  # implementation. Pinned so a later reader does not mistake it for coverage.
  def test_a_one_word_name_is_unchanged
    schema = FraiseQL::Schema.new
    schema.query :users, return_type: "User", returns_list: true, sql_source: "v_user"

    assert_equal "users", schema.to_h["queries"].first["name"]
  end

  # The author may still pass the wire name directly; translation is idempotent, so both
  # idioms land on the same spelling rather than one of them being corrupted.
  def test_a_camelcase_string_is_accepted_unchanged
    schema = FraiseQL::Schema.new
    schema.query "tenantOrders", return_type: "Order", returns_list: true, sql_source: "v_order"

    assert_equal "tenantOrders", schema.to_h["queries"].first["name"]
  end
end

# ── Version ───────────────────────────────────────────────────────────────

class VersionTest < Minitest::Test
  def test_version_is_semver
    assert_match(/\A\d+\.\d+\.\d+\z/, FraiseQL::VERSION)
  end
end
