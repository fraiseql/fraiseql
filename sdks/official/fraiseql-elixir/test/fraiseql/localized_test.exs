defmodule FraiseQL.LocalizedTest do
  @moduledoc """
  `localized` authoring (#1527): a String field, input field or mutation argument stored as
  a locale map. These tests follow the declaration to the exported JSON, and refuse it on
  anything but a String where it is declared, as the Python SDK refuses `Localized[int]`.
  """
  use ExUnit.Case

  defmodule LocalizedSchema do
    use FraiseQL.Schema

    fraiseql_type "Product", sql_source: "v_product" do
      field :id, :id, nullable: false
      field :name, :string, nullable: true, localized: true
      field :sku, :string, nullable: false
    end

    fraiseql_type "CreateProductInput", is_input: true do
      field :name, :string, nullable: false, localized: true
    end

    fraiseql_type "Article", sql_source: "v_article", crud: true do
      field :id, :integer, nullable: false
      field :title, :string, nullable: false, localized: true
    end

    fraiseql_mutation :create_product,
      return_type: "Product",
      sql_source: "fn_create_product",
      operation: "insert" do
      argument :sku, :string, nullable: false
      argument :display_name, :string, nullable: true, localized: true
    end
  end

  defp document, do: LocalizedSchema |> FraiseQL.SchemaExporter.export() |> Jason.decode!()

  defp named(items, name), do: Enum.find(items, &(&1["name"] == name)) || flunk("#{name} absent")

  defp fields_of(section, name) do
    document()
    |> Map.get(section, [])
    |> named(name)
    |> Map.fetch!("fields")
    |> Map.new(&{&1["name"], &1})
  end

  defp inputs, do: Map.get(document(), "input_types", []) ++ Map.fetch!(document(), "types")

  test "a localized field and input field are exported as localized" do
    fields = fields_of("types", "Product")
    assert fields["name"]["localized"] == true
    refute Map.has_key?(fields["sku"], "localized")

    input = inputs() |> named("CreateProductInput") |> Map.fetch!("fields") |> named("name")
    assert input["localized"] == true
  end

  test "a localized mutation argument is exported as localized" do
    arguments = document() |> Map.fetch!("mutations") |> named("createProduct") |> Map.fetch!("arguments")

    assert named(arguments, "displayName") ==
             %{"name" => "displayName", "type" => "String", "nullable" => true, "localized" => true}

    refute Map.has_key?(named(arguments, "sku"), "localized")
  end

  test "a CRUD input field is localized as its field is" do
    for input <- ["CreateArticleInput", "UpdateArticleInput"] do
      title = inputs() |> named(input) |> Map.fetch!("fields") |> named("title")
      assert title["localized"] == true, input
    end
  end

  test "localized is refused on a field that is not a String" do
    assert_raise ArgumentError, ~r/price.*only a String can be localized/, fn ->
      defmodule LocalizedFloat do
        use FraiseQL.Schema

        fraiseql_type "Priced", sql_source: "v_priced" do
          field :price, :float, nullable: false, localized: true
        end
      end
    end
  end

  test "localized is refused on an argument that is not a String" do
    assert_raise ArgumentError, ~r/amount.*only a String can be localized/, fn ->
      defmodule LocalizedFloatArgument do
        use FraiseQL.Schema

        fraiseql_mutation :set_price,
          return_type: "Product",
          sql_source: "fn_set_price",
          operation: "update" do
          argument :amount, :float, nullable: false, localized: true
        end
      end
    end
  end
end
