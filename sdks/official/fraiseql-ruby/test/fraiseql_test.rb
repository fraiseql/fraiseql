# frozen_string_literal: true

require_relative "test_helper"

# ── Type definition tests ─────────────────────────────────────────────────

class Product
  include FraiseQL::Type

  fraiseql_field :id,    :ID,     required: true
  fraiseql_field :name,  :String, required: true, description: "Product name"
  fraiseql_field :price, :Float,  required: true, deprecated: true
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

  def test_deprecated_field_flagged
    schema = Product.to_fraiseql_schema
    price_field = schema[:fields].find { |f| f[:name] == "price" }
    assert_equal true, price_field[:deprecated]
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

# ── Version ───────────────────────────────────────────────────────────────

class VersionTest < Minitest::Test
  def test_version_is_semver
    assert_match(/\A\d+\.\d+\.\d+\z/, FraiseQL::VERSION)
  end
end
