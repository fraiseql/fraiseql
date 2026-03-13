defmodule FraiseQL.ExportTypesTest do
  @moduledoc false
  use ExUnit.Case

  setup do
    FraiseQL.Schema.Legacy.start_link([])
    :ok
  end

  test "exports minimal schema with single type" do
    FraiseQL.Schema.Legacy.register_type("User", %{
      "id" => %{type: "ID", nullable: false},
      "name" => %{type: "String", nullable: false},
      "email" => %{type: "String", nullable: false}
    }, "User in the system")

    json = FraiseQL.Schema.Legacy.export_types(true)
    parsed = Jason.decode!(json)

    assert parsed["types"] != nil
    assert is_list(parsed["types"])
    assert length(parsed["types"]) == 1

    refute Map.has_key?(parsed, "queries")
    refute Map.has_key?(parsed, "mutations")
    refute Map.has_key?(parsed, "observers")

    user_def = List.first(parsed["types"])
    assert user_def["name"] == "User"
    assert user_def["description"] == "User in the system"

    FraiseQL.Schema.Legacy.reset()
  end

  test "exports minimal schema with multiple types" do
    FraiseQL.Schema.Legacy.register_type("User", %{
      "id" => %{type: "ID", nullable: false},
      "name" => %{type: "String", nullable: false}
    })

    FraiseQL.Schema.Legacy.register_type("Post", %{
      "id" => %{type: "ID", nullable: false},
      "title" => %{type: "String", nullable: false}
    })

    json = FraiseQL.Schema.Legacy.export_types(true)
    parsed = Jason.decode!(json)

    assert length(parsed["types"]) == 2

    type_names = Enum.map(parsed["types"], & &1["name"])
    assert "User" in type_names
    assert "Post" in type_names

    FraiseQL.Schema.Legacy.reset()
  end

  test "does not include queries in minimal export" do
    FraiseQL.Schema.Legacy.register_type("User", %{
      "id" => %{type: "ID", nullable: false}
    })

    json = FraiseQL.Schema.Legacy.export_types(true)
    parsed = Jason.decode!(json)

    assert parsed["types"] != nil
    refute Map.has_key?(parsed, "queries")
    refute Map.has_key?(parsed, "mutations")

    FraiseQL.Schema.Legacy.reset()
  end

  test "exports compact format when pretty is false" do
    FraiseQL.Schema.Legacy.register_type("User", %{
      "id" => %{type: "ID", nullable: false}
    })

    compact = FraiseQL.Schema.Legacy.export_types(false)
    pretty = FraiseQL.Schema.Legacy.export_types(true)

    assert Jason.decode!(compact)["types"] != nil
    assert String.length(compact) <= String.length(pretty)

    FraiseQL.Schema.Legacy.reset()
  end

  test "exports pretty format when pretty is true" do
    FraiseQL.Schema.Legacy.register_type("User", %{
      "id" => %{type: "ID", nullable: false}
    })

    json = FraiseQL.Schema.Legacy.export_types(true)
    assert String.contains?(json, "\n")
    assert Jason.decode!(json)["types"] != nil

    FraiseQL.Schema.Legacy.reset()
  end

  test "exports types to file" do
    FraiseQL.Schema.Legacy.register_type("User", %{
      "id" => %{type: "ID", nullable: false},
      "name" => %{type: "String", nullable: false}
    })

    tmp_file = "/tmp/fraiseql_types_test_elixir.json"

    if File.exists?(tmp_file) do
      File.rm(tmp_file)
    end

    FraiseQL.Schema.Legacy.export_types_file(tmp_file)

    assert File.exists?(tmp_file)

    content = File.read!(tmp_file)
    parsed = Jason.decode!(content)

    assert parsed["types"] != nil
    assert length(parsed["types"]) == 1

    File.rm(tmp_file)
    FraiseQL.Schema.Legacy.reset()
  end

  test "handles empty schema gracefully" do
    json = FraiseQL.Schema.Legacy.export_types(true)
    parsed = Jason.decode!(json)

    assert parsed["types"] != nil
    assert is_list(parsed["types"])
    assert length(parsed["types"]) == 0

    FraiseQL.Schema.Legacy.reset()
  end
end
