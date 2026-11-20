defmodule FraiseQL.ScopedLikeParentTest do
  @moduledoc """
  `scoped_like_parent` authoring: a field that embeds a type whose own query is scoped by
  `inject_params` is exported with `scoped_like_parent: true`; a field that does not
  declare it is exported without the key. An input field cannot declare it.
  """
  use ExUnit.Case

  alias FraiseQL.SchemaExporter

  defmodule ScopedSchema do
    use FraiseQL.Schema

    fraiseql_type "OrderLine", sql_source: "v_order_line" do
      field :id, :id, nullable: false
      field :sku, :string, nullable: false
    end

    fraiseql_type "Order", sql_source: "v_order" do
      field :id, :id, nullable: false
      field :lines, "[OrderLine!]", nullable: false, scoped_like_parent: true
      field :first_line, "OrderLine", nullable: true
      field :last_line, "OrderLine", nullable: true, scoped_like_parent: false
    end

    fraiseql_type "OrderInput", is_input: true do
      field :line_id, :id, nullable: false
    end
  end

  defp fields_of(type) do
    ScopedSchema
    |> SchemaExporter.export()
    |> Jason.decode!()
    |> Map.fetch!("types")
    |> Enum.find(&(&1["name"] == type))
    |> Map.fetch!("fields")
    |> Map.new(&{&1["name"], &1})
  end

  test "scoped_like_parent is exported only on the field that declares it" do
    order = fields_of("Order")

    assert order["lines"]["scoped_like_parent"] == true
    refute Map.has_key?(order["id"], "scoped_like_parent")
    refute Map.has_key?(order["firstLine"], "scoped_like_parent")
    refute Map.has_key?(order["lastLine"], "scoped_like_parent")

    for type <- ["OrderLine", "OrderInput"], {_name, field} <- fields_of(type) do
      refute Map.has_key?(field, "scoped_like_parent"), type
    end
  end

  test "a non-boolean scoped_like_parent is refused" do
    assert_raise ArgumentError, ~r/lines: scoped_like_parent: must be true or false/, fn ->
      defmodule NonBooleanScoped do
        use FraiseQL.Schema

        fraiseql_type "Order", sql_source: "v_order" do
          field :lines, "[OrderLine!]", nullable: false, scoped_like_parent: "yes"
        end
      end
    end
  end

  test "an input field cannot declare scoped_like_parent" do
    assert_raise ArgumentError, ~r/"OrderInput".*"lineId".*scoped_like_parent/, fn ->
      defmodule ScopedInput do
        use FraiseQL.Schema

        fraiseql_type "OrderInput", is_input: true do
          field :line_id, :id, nullable: false, scoped_like_parent: true
        end
      end
    end
  end
end
