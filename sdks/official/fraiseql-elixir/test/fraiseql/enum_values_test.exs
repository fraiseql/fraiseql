defmodule FraiseQL.EnumValuesTest do
  @moduledoc false
  use ExUnit.Case

  alias FraiseQL.SchemaExporter

  # #1611: a member is stored as the value its declaration gives it. A list of names stores
  # each as its name; a keyword list maps each member to its value, a string or an integer,
  # as Ecto.Enum writes `values: [low: 1, high: 3]`. A keyword list used to crash the export.
  defmodule ValuedSchema do
    use FraiseQL.Schema

    fraiseql_enum("OrderStatus", values: ["PENDING", "SHIPPED"])

    fraiseql_enum("Delivery",
      values: [PENDING: "pending", IN_TRANSIT: "transit", SHIPPED: "SHIPPED"]
    )

    fraiseql_enum("Priority", values: [LOW: 1, HIGH: 3])
  end

  defp exported(name) do
    ValuedSchema
    |> SchemaExporter.export()
    |> Jason.decode!()
    |> Map.fetch!("enums")
    |> Enum.find(&(&1["name"] == name))
    |> Map.fetch!("values")
  end

  test "a list of names stores each member as its name" do
    assert exported("OrderStatus") == [%{"name" => "PENDING"}, %{"name" => "SHIPPED"}]
  end

  test "a keyword list maps each member to its value" do
    assert exported("Delivery") == [
             %{"name" => "PENDING", "value" => "pending"},
             %{"name" => "IN_TRANSIT", "value" => "transit"},
             %{"name" => "SHIPPED"}
           ]

    assert exported("Priority") == [
             %{"name" => "LOW", "value" => 1},
             %{"name" => "HIGH", "value" => 3}
           ]
  end

  test "a value that is neither a string nor an integer is refused, naming the member" do
    assert_raise ArgumentError, ~r/Ratio\.HALF/, fn ->
      FraiseQL.EnumDefinition.members!("Ratio", HALF: 0.5)
    end
  end
end
