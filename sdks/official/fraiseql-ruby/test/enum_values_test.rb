# frozen_string_literal: true

require "test_helper"

# #1611: a member is stored as the value its declaration gives it. An Array names members
# stored as their names; a Hash maps each name to its value, a String or an Integer, as
# Rails writes `enum status: { pending: 0, shipped: 1 }`. A Hash used to export members
# named `["pending", 0]`.
class EnumValuesTest < Minitest::Test
  def values(definition)
    definition["values"]
  end

  def test_an_array_names_members_stored_as_their_names
    schema = FraiseQL::Schema.new

    assert_equal [{ "name" => "PENDING" }, { "name" => "SHIPPED" }],
                 values(schema.enum("OrderStatus", %w[PENDING SHIPPED]))
  end

  def test_a_hash_maps_each_member_to_its_value
    schema = FraiseQL::Schema.new

    assert_equal [{ "name" => "PENDING", "value" => "pending" },
                  { "name" => "IN_TRANSIT", "value" => "transit" },
                  { "name" => "SHIPPED" }],
                 values(schema.enum("OrderStatus",
                                    { PENDING: "pending", IN_TRANSIT: "transit", SHIPPED: "SHIPPED" }))
    assert_equal [{ "name" => "LOW", "value" => 1 }, { "name" => "HIGH", "value" => 3 }],
                 values(schema.enum("Priority", { LOW: 1, HIGH: 3 }))
  end

  def test_a_value_that_is_neither_a_string_nor_an_integer_is_refused
    schema = FraiseQL::Schema.new

    error = assert_raises(ArgumentError) { schema.enum("Ratio", { HALF: 0.5 }) }
    assert_includes error.message, "Ratio.HALF"
  end
end
