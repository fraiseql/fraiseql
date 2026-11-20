defmodule FraiseQL.RetryOnAbortTest do
  @moduledoc """
  `retry_on_abort` authoring (#1936): a mutation that opts out of transaction retry is
  exported with `retry_on_abort: false`; one that does not declare it is exported without
  the key, so it follows the server.
  """
  use ExUnit.Case

  alias FraiseQL.SchemaExporter

  defmodule RetrySchema do
    use FraiseQL.Schema

    fraiseql_type "Charge", sql_source: "v_charge" do
      field :id, :id, nullable: false
    end

    fraiseql_mutation(:charge_card,
      return_type: "Charge",
      sql_source: "fn_charge_card",
      operation: "insert",
      retry_on_abort: false
    )

    fraiseql_mutation(:refund,
      return_type: "Charge",
      sql_source: "fn_refund",
      operation: "insert"
    )
  end

  test "retry_on_abort is exported only when it is declared" do
    mutations =
      RetrySchema
      |> SchemaExporter.export()
      |> Jason.decode!()
      |> Map.fetch!("mutations")
      |> Map.new(&{&1["name"], &1})

    assert mutations["chargeCard"]["retry_on_abort"] == false
    refute Map.has_key?(mutations["refund"], "retry_on_abort")
  end

  test "a non-boolean retry_on_abort is refused" do
    assert_raise ArgumentError, ~r/retry_on_abort: must be true or false/, fn ->
      FraiseQL.Schema.__validate_retry_on_abort__!("payout", "no")
    end
  end
end
