# frozen_string_literal: true

require "test_helper"

# `:local_datetime` declares a date and time with no offset, a PostgreSQL `timestamp`.
# Ruby's `Time` and `DateTime` carry an offset, so `:datetime` stays `DateTime`. Without
# the entry the symbol PascalCases to `LocalDatetime`, which the compiler refuses.
class LocalDateTimeTest < Minitest::Test
  def test_the_local_datetime_symbol_is_the_built_in_scalar_and_datetime_stays_datetime
    assert_equal "LocalDateTime", FraiseQL::Schema.graphql_type(:local_datetime)
    assert_equal "DateTime", FraiseQL::Schema.graphql_type(:datetime)
  end

  def test_a_local_datetime_field_and_argument_export_as_local_date_time
    schema = FraiseQL::Schema.new
    schema.type "Shift", sql_source: "v_shift" do |t|
      t.field :id, :id, nullable: false
      t.field :starts_at, :local_datetime, nullable: false
      t.field :ends_at, :local_datetime, nullable: true
    end
    schema.query :shifts, return_type: "Shift", returns_list: true, sql_source: "v_shift" do |q|
      q.argument :starts_after, :local_datetime, nullable: false
    end

    exported = schema.to_h
    fields = exported["types"].first["fields"].to_h { |f| [f["name"], f["type"]] }
    assert_equal({ "id" => "ID", "startsAt" => "LocalDateTime", "endsAt" => "LocalDateTime" }, fields)
    assert_equal "LocalDateTime", exported["queries"].first["arguments"].first["type"]
  end
end
