# frozen_string_literal: true

require "test_helper"
require "tmpdir"

# The README's Quick Start has always opened with `require "fraiseql"` and
# `FraiseQL::Schema.new`. Neither existed: there was no `lib/fraiseql.rb`, so the require
# raised LoadError, and no `Schema` class, so the next line raised NameError (#853). The
# only working entry point was the `FraiseQL::Type` mixin, whose `to_fraiseql_schema`
# omitted the required `nullable` key and could not be compiled either (#854).
# Named rather than anonymous: `FraiseQL::Type.included` derives the type name from
# `base.name`, which is nil for a `Class.new` and raises there.
class MixinProduct
  include FraiseQL::Type

  fraiseql_field :id, :ID, required: true
  fraiseql_field :description, :String, required: false
end

class SchemaTest < Minitest::Test
  def test_require_fraiseql_resolves
    # The failure this pins is a LoadError on the documented first line, so the assertion
    # has to be that the entry point loads at all.
    assert defined?(FraiseQL::Schema), "require \"fraiseql\" must define FraiseQL::Schema"
    assert defined?(FraiseQL::VERSION)
  end

  def test_type_fields_carry_nullable
    schema = FraiseQL::Schema.new
    schema.type "User", sql_source: "v_user" do |t|
      t.field :id, :id, nullable: false
      t.field :name, :string, nullable: true
    end

    fields = schema.to_h["types"].first["fields"]

    assert_equal({ "name" => "id", "type" => "ID", "nullable" => false }, fields[0])
    assert_equal({ "name" => "name", "type" => "String", "nullable" => true }, fields[1])
  end

  def test_inject_is_emitted_as_nested_inject_params
    schema = FraiseQL::Schema.new
    schema.query "tenantOrders", return_type: "Order", returns_list: true,
                                 sql_source: "v_order",
                                 inject: { "tenant_id" => "jwt:tenant_id" }

    query = schema.to_h["queries"].first

    assert_equal({ "tenant_id" => { "source" => "jwt", "claim" => "tenant_id" } },
                 query["inject_params"])
    refute query.key?("inject"), "`inject` is a key the compiler does not read"
  end

  def test_malformed_inject_source_is_refused
    schema = FraiseQL::Schema.new

    error = assert_raises(ArgumentError) do
      schema.query "orders", return_type: "Order", inject: { "tenant_id" => "tenant_id" }
    end
    assert_match(/<source>:<claim>/, error.message)
  end

  def test_input_type_must_not_declare_sql_source
    schema = FraiseQL::Schema.new

    error = assert_raises(ArgumentError) do
      schema.type "CreateUserInput", is_input: true, sql_source: "v_bogus"
    end
    assert_match(/input type must not declare sql_source/, error.message)
  end

  def test_empty_sections_are_omitted_not_null
    schema = FraiseQL::Schema.new
    schema.type("User", sql_source: "v_user") { |t| t.field :id, :id, nullable: false }

    document = schema.to_h

    # A `null` array is rejected by the compiler with `invalid type: null, expected a
    # sequence` and no key name.
    refute document.key?("mutations")
    refute document.key?("enums")
  end

  def test_export_json_writes_a_file
    schema = FraiseQL::Schema.new
    schema.type("User", sql_source: "v_user") { |t| t.field :id, :id, nullable: false }

    Dir.mktmpdir do |dir|
      path = File.join(dir, "schema.json")
      schema.export_json(path)

      parsed = JSON.parse(File.read(path))
      assert_equal "2.0.0", parsed["version"]
      assert_equal "User", parsed["types"].first["name"]
    end
  end

  # #854: the mixin path omitted `nullable` on every field, so any schema built from it
  # was rejected by the compiler.
  def test_mixin_to_fraiseql_schema_carries_nullable
    fields = MixinProduct.to_fraiseql_schema[:fields]

    assert_equal false, fields[0][:nullable]
    assert_equal true, fields[1][:nullable]
    refute fields.any? { |f| f.key?(:deprecated) },
           "`deprecated` is not a member of IntermediateField and is now refused by the compiler"
  end
end
