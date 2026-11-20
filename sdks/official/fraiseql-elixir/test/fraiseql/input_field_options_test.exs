defmodule FraiseQL.InputFieldOptionsTest do
  @moduledoc """
  An input object is an argument shape. A field of an `is_input` type takes a description,
  a deprecation and `localized`; every other field option configures an output field and
  is refused where the type is declared, naming the type, the field and each option.
  """
  use ExUnit.Case

  alias FraiseQL.{CrudGenerator, FieldDefinition, SchemaExporter}

  # {option, a field type the option is valid on, a value that declares it}
  @refused [
    {:requires_scope, :string, "read:order.note"},
    {:requires_scopes, :string, ["read:order.note"]},
    {:computed, :string, true},
    {:vector_config, :vector, [dimensions: 3]},
    {:vector_distance, :float, :embedding},
    {:scoped_like_parent, :string, true}
  ]

  @allowed [:name, :type, :nullable, :description, :deprecated, :localized]

  @refused_options Enum.map(@refused, &elem(&1, 0))

  # Declares `OrderInput` with one field, `note`, in a fresh schema module.
  defp declare_input(type, field_opts) do
    body =
      quote do
        use FraiseQL.Schema

        fraiseql_type "OrderInput", is_input: true do
          field :note, unquote(type), unquote(field_opts)
        end
      end

    name = Module.concat(__MODULE__, "Declared#{System.unique_integer([:positive])}")
    {:module, module, _bytecode, _result} = Module.create(name, body, __ENV__)
    module
  end

  for {option, type, value} <- @refused do
    test "an input field cannot declare #{option}" do
      error =
        assert_raise ArgumentError, fn ->
          declare_input(unquote(type), [{unquote(option), unquote(value)}])
        end

      assert error.message ==
               ~s(fraiseql_type "OrderInput": input field "note": #{unquote(option)} ) <>
                 "applies to output fields only; an input field cannot declare it"
    end
  end

  test "every offending option on one field is named, sorted" do
    error =
      assert_raise ArgumentError, fn ->
        declare_input(:vector, vector_config: [dimensions: 3], requires_scope: "read:order.note")
      end

    assert error.message ==
             ~s(fraiseql_type "OrderInput": input field "note": requires_scope, vector_config ) <>
               "applies to output fields only; an input field cannot declare it"
  end

  test "an input field takes description, deprecated and localized" do
    module =
      declare_input(:string,
        nullable: true,
        description: "A note for the warehouse",
        deprecated: "use memo",
        localized: true
      )

    [type] = module |> SchemaExporter.export() |> Jason.decode!() |> Map.fetch!("types")

    assert type["is_input"] == true

    assert type["fields"] == [
             %{
               "name" => "note",
               "type" => "String",
               "nullable" => true,
               "description" => "A note for the warehouse",
               "deprecated" => %{"reason" => "use memo"},
               "localized" => true
             }
           ]
  end

  test "an input type built as a struct is refused when it is exported" do
    defmodule HandBuilt do
      def __fraiseql_types__ do
        [
          %FraiseQL.TypeDefinition{
            name: "OrderInput",
            sql_source: nil,
            is_input: true,
            fields: [
              %FraiseQL.FieldDefinition{name: "note", type: "String", requires_scope: "read:note"}
            ]
          }
        ]
      end

      def __fraiseql_queries__, do: []
      def __fraiseql_mutations__, do: []
    end

    assert_raise ArgumentError,
                 ~r/"OrderInput": input field "note": requires_scope applies to output/,
                 fn -> SchemaExporter.export(HandBuilt) end
  end

  test "every FieldDefinition key is either refused or allowed on an input field" do
    keys = FieldDefinition.__struct__() |> Map.keys() |> List.delete(:__struct__)

    assert keys -- (@refused_options ++ @allowed) == [],
           "a FieldDefinition key is in neither set: decide whether an input field may " <>
             "declare it, then add it to @refused (and to the SDK's refusal) or to @allowed"

    assert (@refused_options ++ @allowed) -- keys == [],
           "a classified name is no longer a FieldDefinition key: remove it"

    assert Enum.filter(@refused_options, &(&1 in @allowed)) == [],
           "a key is both refused and allowed"
  end

  defmodule CrudSchema do
    use FraiseQL.Schema

    fraiseql_type "Line", sql_source: "v_line" do
      field :id, :id, nullable: false
    end

    fraiseql_type "Document", sql_source: "v_document", crud: true do
      field :id, :id, nullable: false
      field :title, :string, nullable: false, requires_scope: "read:document.title"
      field :body, :string, nullable: true, requires_scopes: ["read:document.body"]
      field :slug, :string, nullable: false, computed: true
      field :embedding, :vector, nullable: false, vector_config: [dimensions: 3]
      field :similarity, :float, nullable: false, vector_distance: :embedding
      field :lines, "[Line!]", nullable: false, scoped_like_parent: true
    end
  end

  describe "a crud type whose fields carry output-only options" do
    test "declares them on the type itself" do
      document = Enum.find(CrudSchema.__fraiseql_types__(), &(&1.name == "Document"))
      declared = for f <- document.fields, o <- @refused_options, declares?(f, o), do: o

      assert Enum.sort(declared) == Enum.sort(@refused_options)
    end

    test "generates input objects whose fields carry none of them" do
      inputs = Enum.filter(CrudSchema.__fraiseql_types__(), & &1.is_input)

      assert Enum.map(inputs, & &1.name) == ["CreateDocumentInput", "UpdateDocumentInput"]

      for input <- inputs, field <- input.fields, option <- @refused_options do
        refute declares?(field, option), "#{input.name}.#{field.name} carries #{option}"
      end
    end

    test "exports input objects without the keys" do
      types = CrudSchema |> SchemaExporter.export() |> Jason.decode!() |> Map.fetch!("types")
      inputs = Enum.filter(types, & &1["is_input"])

      assert length(inputs) == 2

      for input <- inputs, field <- input["fields"] do
        assert Map.keys(field) -- ["name", "type", "nullable"] == [],
               "#{input["name"]}.#{field["name"]}"
      end
    end

    test "the generator strips them from a type handed to it directly" do
      document = Enum.find(CrudSchema.__fraiseql_types__(), &(&1.name == "Document"))
      {_queries, _mutations, inputs} = CrudGenerator.generate(document)

      for input <- inputs do
        assert FraiseQL.Schema.__validate_input_fields__!(input.name, true, input.fields) == :ok
        refute Enum.any?(input.fields, &(&1.name == "slug"))
      end
    end
  end

  defp declares?(field, option), do: Map.fetch!(field, option) not in [nil, false]
end
