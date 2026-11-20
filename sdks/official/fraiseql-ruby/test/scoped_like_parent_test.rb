# frozen_string_literal: true

require "test_helper"

# `scoped_like_parent` on a field that embeds a scoped type. A type whose own query is
# scoped by `inject` does not compile inside another type's document unless the embedding
# field declares it scoped like its parent. The compiler checks the claim; the SDK's part
# is to carry the declaration, and only where it was made.
class ScopedLikeParentTest < Minitest::Test
  def fields_of(schema, type_name)
    type = schema.to_h["types"].find { |t| t["name"] == type_name } ||
           flunk("#{type_name} is absent from the exported types")
    type["fields"].to_h { |f| [f["name"], f] }
  end

  def test_the_declaration_is_exported_only_on_the_field_that_makes_it
    schema = FraiseQL::Schema.new
    schema.type "OrderLine", sql_source: "v_order_line" do |t|
      t.field :id, :id, nullable: false
      t.field :order, "Order", nullable: false
    end
    schema.type "Order", sql_source: "v_order" do |t|
      t.field :id, :id, nullable: false
      t.field :lines, "[OrderLine!]", nullable: false, scoped_like_parent: true
      t.field :first_line, "OrderLine", nullable: true
      t.field :last_line, "OrderLine", nullable: true, scoped_like_parent: false
    end

    order = fields_of(schema, "Order")
    assert_equal({ "name" => "lines", "type" => "[OrderLine!]", "nullable" => false,
                   "scoped_like_parent" => true }, order["lines"])
    refute order["id"].key?("scoped_like_parent"), "a sibling field carries no key"
    refute order["firstLine"].key?("scoped_like_parent"), "a sibling embed carries no key"
    refute order["lastLine"].key?("scoped_like_parent"), "false declares nothing"
    fields_of(schema, "OrderLine").each do |name, field|
      refute field.key?("scoped_like_parent"), "OrderLine.#{name} declared nothing"
    end
  end

  def test_a_non_boolean_is_refused
    ["yes", 1, :yes].each do |value|
      error = assert_raises(ArgumentError) do
        FraiseQL::Schema.new.type "Order", sql_source: "v_order" do |t|
          t.field :lines, "[OrderLine!]", nullable: false, scoped_like_parent: value
        end
      end
      assert_match(/scoped_like_parent: must be true or false/, error.message)
    end
  end

  def test_an_input_field_cannot_declare_it
    error = assert_raises(ArgumentError) do
      FraiseQL::Schema.new.type "OrderInput", is_input: true do |t|
        t.field :line_id, :id, nullable: false, scoped_like_parent: true
      end
    end
    assert_match(/scoped_like_parent: is not valid on an input type's field/, error.message)
  end

  def test_a_crud_input_object_does_not_inherit_it
    schema = FraiseQL::Schema.new
    schema.type "Shipment", sql_source: "v_shipment", crud: true do |t|
      t.field :id, :int, nullable: false
      t.field :order, "Order", nullable: false, scoped_like_parent: true
    end

    assert_equal true, fields_of(schema, "Shipment")["order"]["scoped_like_parent"]
    %w[CreateShipmentInput UpdateShipmentInput].each do |input|
      fields_of(schema, input).each_value do |field|
        refute field.key?("scoped_like_parent"), "#{input}.#{field['name']}"
      end
    end
  end
end
