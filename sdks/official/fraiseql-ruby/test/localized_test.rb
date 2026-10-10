# frozen_string_literal: true

require "test_helper"

# `localized` authoring (#1527): a String field, input field or mutation argument stored as
# a locale map. These tests follow the declaration to the exported hash, and refuse it on
# anything but a String where it is declared, as the Python SDK refuses `Localized[int]`.
class LocalizedTest < Minitest::Test
  def named(items, name)
    items.find { |item| item["name"] == name } || flunk("#{name} is absent from #{items.inspect}")
  end

  def test_a_localized_field_and_input_field_are_exported_as_localized
    schema = FraiseQL::Schema.new
    schema.type "Product", sql_source: "v_product" do |t|
      t.field :id, :id, nullable: false
      t.field :name, :string, nullable: true, localized: true
      t.field :sku, :string, nullable: false
    end
    schema.type "CreateProductInput", is_input: true do |t|
      t.field :name, :string, nullable: false, localized: true
    end

    document = schema.to_h
    fields = named(document["types"], "Product")["fields"]
    assert_equal true, named(fields, "name")["localized"]
    refute named(fields, "sku").key?("localized"), "an unlocalized field carries no key"
    inputs = document["types"] + document.fetch("input_types", [])
    assert_equal true, named(named(inputs, "CreateProductInput")["fields"], "name")["localized"]
  end

  def test_a_localized_mutation_argument_is_exported_as_localized
    schema = FraiseQL::Schema.new
    schema.mutation :create_product, return_type: "Product", sql_source: "fn_create_product",
                                     operation: "insert" do |m|
      m.argument :sku, :string, nullable: false
      m.argument :display_name, :string, nullable: true, localized: true
    end

    arguments = named(schema.to_h["mutations"], "createProduct")["arguments"]
    assert_equal({ "name" => "displayName", "type" => "String", "nullable" => true, "localized" => true },
                 named(arguments, "displayName"))
    refute named(arguments, "sku").key?("localized"), "an unlocalized argument carries no key"
  end

  def test_a_crud_input_field_is_localized_as_its_field_is
    schema = FraiseQL::Schema.new
    schema.type "Article", sql_source: "v_article", crud: true do |t|
      t.field :id, :int, nullable: false
      t.field :title, :string, nullable: false, localized: true
    end

    types = schema.to_h["types"]
    %w[CreateArticleInput UpdateArticleInput].each do |input|
      assert_equal true, named(named(types, input)["fields"], "title")["localized"], input
    end
  end

  def test_localized_is_refused_on_a_field_that_is_not_a_string
    schema = FraiseQL::Schema.new
    error = assert_raises(ArgumentError) do
      schema.type "Priced", sql_source: "v_priced" do |t|
        t.field :price, :float, nullable: false, localized: true
      end
    end
    assert_includes error.message, "price"
    assert_includes error.message, "only a String can be localized"
  end

  def test_localized_is_refused_on_an_argument_that_is_not_a_string
    schema = FraiseQL::Schema.new
    error = assert_raises(ArgumentError) do
      schema.mutation :set_price, return_type: "Product", sql_source: "fn_set_price",
                                  operation: "update" do |m|
        m.argument :amount, :float, nullable: false, localized: true
      end
    end
    assert_includes error.message, "amount"
    assert_includes error.message, "only a String can be localized"
  end
end
