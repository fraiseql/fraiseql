# frozen_string_literal: true

require "test_helper"

# `:bigint` declares a signed 64-bit integer, served as the string of its digits. Ruby's
# Integer has no width and is the type for small values, so `:int` stays `Int`, which is
# 32 bits and refuses a value past that at read time.
class BigIntTest < Minitest::Test
  def test_the_bigint_symbol_is_the_built_in_scalar_and_int_stays_int
    assert_equal "BigInt", FraiseQL::Schema.graphql_type(:bigint)
    assert_equal "Int", FraiseQL::Schema.graphql_type(:int)
    assert_equal "Int", FraiseQL::Schema.graphql_type(:integer)
  end

  def test_a_bigint_field_and_argument_export_as_big_int
    schema = FraiseQL::Schema.new
    schema.type "Ledger", sql_source: "v_ledger" do |t|
      t.field :id, :id, nullable: false
      t.field :amount, :bigint, nullable: false
      t.field :balance, :bigint, nullable: true
    end
    schema.query :ledgers, return_type: "Ledger", returns_list: true, sql_source: "v_ledger" do |q|
      q.argument :account_number, :bigint, nullable: false
    end

    exported = schema.to_h
    fields = exported["types"].first["fields"].to_h { |f| [f["name"], f["type"]] }
    assert_equal({ "id" => "ID", "amount" => "BigInt", "balance" => "BigInt" }, fields)
    assert_equal "BigInt", exported["queries"].first["arguments"].first["type"]
  end
end
