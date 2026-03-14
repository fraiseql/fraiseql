defmodule FraiseQL.SchemaExporterTest do
  @moduledoc false
  use ExUnit.Case

  alias FraiseQL.SchemaExporter

  # Use the shared fixture schema defined in test/support/fixture_schema.ex
  alias FraiseQL.Test.FixtureSchema

  # Simple schema used for focused unit tests
  defmodule SimpleSchema do
    use FraiseQL.Schema

    fraiseql_type "Author", sql_source: "v_author", description: "A blog author" do
      field :id, :id, nullable: false
      field :name, :string, nullable: false
      field :bio, :string, nullable: true
    end

    fraiseql_query :authors,
      return_type: "Author",
      returns_list: true,
      sql_source: "v_author"
  end

  defmodule EmptySchema do
    use FraiseQL.Schema
  end

  defmodule ScopedSchema do
    use FraiseQL.Schema

    fraiseql_type "User", sql_source: "v_user" do
      field :id, :id, nullable: false
      field :email, :string, nullable: false, requires_scope: "read:user.email"
      field :roles, :string, nullable: true, requires_scopes: ["admin:read", "read:roles"]
    end
  end

  defmodule CachedQuerySchema do
    use FraiseQL.Schema

    fraiseql_query :posts,
      return_type: "Post",
      returns_list: true,
      sql_source: "v_post",
      cache_ttl_seconds: 300
  end

  defmodule NoDescSchema do
    use FraiseQL.Schema

    fraiseql_type "Post", sql_source: "v_post" do
      field :id, :id, nullable: false
    end
  end

  # ---------------------------------------------------------------------------
  # to_intermediate_schema/1
  # ---------------------------------------------------------------------------

  test "to_intermediate_schema returns IntermediateSchema struct" do
    schema = SchemaExporter.to_intermediate_schema(SimpleSchema)
    assert %FraiseQL.IntermediateSchema{} = schema
    assert schema.version == "2.0.0"
    assert length(schema.types) == 1
    assert length(schema.queries) == 1
    assert schema.mutations == []
  end

  test "type definition is correctly populated" do
    schema = SchemaExporter.to_intermediate_schema(SimpleSchema)
    [type] = schema.types
    assert type.name == "Author"
    assert type.sql_source == "v_author"
    assert type.description == "A blog author"
    assert length(type.fields) == 3
  end

  test "field types are correctly mapped" do
    schema = SchemaExporter.to_intermediate_schema(SimpleSchema)
    [type] = schema.types
    id_field = Enum.find(type.fields, &(&1.name == "id"))
    assert id_field.type == "ID"
    assert id_field.nullable == false
    bio_field = Enum.find(type.fields, &(&1.name == "bio"))
    assert bio_field.nullable == true
  end

  test "raises ArgumentError when module is not a FraiseQL schema" do
    assert_raise ArgumentError, ~r/not a FraiseQL\.Schema module/, fn ->
      SchemaExporter.to_intermediate_schema(String)
    end
  end

  # ---------------------------------------------------------------------------
  # export/2 — JSON output
  # ---------------------------------------------------------------------------

  test "export/2 returns valid JSON string" do
    json = SchemaExporter.export(SimpleSchema)
    assert is_binary(json)
    parsed = Jason.decode!(json)
    assert parsed["version"] == "2.0.0"
    assert is_list(parsed["types"])
    assert is_list(parsed["queries"])
    assert is_list(parsed["mutations"])
  end

  test "type fields are serialised with snake_case keys" do
    json = SchemaExporter.export(SimpleSchema)
    parsed = Jason.decode!(json)
    [type] = parsed["types"]
    assert Map.has_key?(type, "name")
    assert Map.has_key?(type, "sql_source")
    assert Map.has_key?(type, "fields")
  end

  test "query serialised with all required keys" do
    json = SchemaExporter.export(SimpleSchema)
    parsed = Jason.decode!(json)
    [query] = parsed["queries"]
    assert query["name"] == "authors"
    assert query["return_type"] == "Author"
    assert query["returns_list"] == true
    assert query["nullable"] == false
    assert query["sql_source"] == "v_author"
    assert query["arguments"] == []
  end

  test "description is omitted when nil" do
    json = SchemaExporter.export(NoDescSchema)
    parsed = Jason.decode!(json)
    [type] = parsed["types"]
    refute Map.has_key?(type, "description")
  end

  test "description is included when present" do
    json = SchemaExporter.export(SimpleSchema)
    parsed = Jason.decode!(json)
    [type] = parsed["types"]
    assert type["description"] == "A blog author"
  end

  test "requires_scope is included when set on field" do
    json = SchemaExporter.export(ScopedSchema)
    parsed = Jason.decode!(json)
    [type] = parsed["types"]
    email_field = Enum.find(type["fields"], &(&1["name"] == "email"))
    assert email_field["requires_scope"] == "read:user.email"
    refute Map.has_key?(email_field, "requires_scopes")
  end

  test "requires_scopes is included when set on field" do
    json = SchemaExporter.export(ScopedSchema)
    parsed = Jason.decode!(json)
    [type] = parsed["types"]
    roles_field = Enum.find(type["fields"], &(&1["name"] == "roles"))
    assert roles_field["requires_scopes"] == ["admin:read", "read:roles"]
    refute Map.has_key?(roles_field, "requires_scope")
  end

  test "cache_ttl_seconds is included when set on query" do
    json = SchemaExporter.export(CachedQuerySchema)
    parsed = Jason.decode!(json)
    [query] = parsed["queries"]
    assert query["cache_ttl_seconds"] == 300
  end

  test "cache_ttl_seconds is omitted when nil" do
    json = SchemaExporter.export(SimpleSchema)
    parsed = Jason.decode!(json)
    [query] = parsed["queries"]
    refute Map.has_key?(query, "cache_ttl_seconds")
  end

  test "empty schema exports version with empty arrays" do
    json = SchemaExporter.export(EmptySchema)
    parsed = Jason.decode!(json)
    assert parsed["version"] == "2.0.0"
    assert parsed["types"] == []
    assert parsed["queries"] == []
    assert parsed["mutations"] == []
  end

  # ---------------------------------------------------------------------------
  # export_to_file!/3
  # ---------------------------------------------------------------------------

  test "export_to_file! writes JSON to path" do
    path = "/tmp/fraiseql_exporter_test_#{:os.getpid()}.json"
    result = SchemaExporter.export_to_file!(SimpleSchema, path)
    assert result == :ok
    assert File.exists?(path)
    parsed = path |> File.read!() |> Jason.decode!()
    assert parsed["version"] == "2.0.0"
    File.rm!(path)
  end

  test "export_to_file! creates parent directories" do
    path = "/tmp/fraiseql_deep_#{:os.getpid()}/sub/schema.json"
    :ok = SchemaExporter.export_to_file!(SimpleSchema, path)
    assert File.exists?(path)
    File.rm_rf!(Path.dirname(Path.dirname(path)))
  end

  # ---------------------------------------------------------------------------
  # compact vs pretty
  # ---------------------------------------------------------------------------

  test "compact export has no newlines" do
    json = SchemaExporter.export(SimpleSchema, compact: true)
    refute String.contains?(json, "\n")
  end

  test "pretty export has newlines" do
    json = SchemaExporter.export(SimpleSchema)
    assert String.contains?(json, "\n")
  end

  # ---------------------------------------------------------------------------
  # Golden tests — full fixture schema
  # ---------------------------------------------------------------------------

  setup_all do
    json = SchemaExporter.export(FixtureSchema)
    parsed = Jason.decode!(json)
    {:ok, parsed: parsed}
  end

  test "golden: full schema version", %{parsed: parsed} do
    assert parsed["version"] == "2.0.0"
  end

  test "golden: two types in output", %{parsed: parsed} do
    assert length(parsed["types"]) == 2
  end

  test "golden: Author type has 4 fields", %{parsed: parsed} do
    author = Enum.find(parsed["types"], &(&1["name"] == "Author"))
    assert length(author["fields"]) == 4
  end

  test "golden: Post type has no description", %{parsed: parsed} do
    post = Enum.find(parsed["types"], &(&1["name"] == "Post"))
    refute Map.has_key?(post, "description")
  end

  test "golden: authors query returns_list true", %{parsed: parsed} do
    q = Enum.find(parsed["queries"], &(&1["name"] == "authors"))
    assert q["returns_list"] == true
  end

  test "golden: author query has one argument", %{parsed: parsed} do
    q = Enum.find(parsed["queries"], &(&1["name"] == "author"))
    assert length(q["arguments"]) == 1
  end

  test "golden: author query argument type is ID", %{parsed: parsed} do
    q = Enum.find(parsed["queries"], &(&1["name"] == "author"))
    [arg] = q["arguments"]
    assert arg["type"] == "ID"
    assert arg["name"] == "id"
    assert arg["nullable"] == false
  end

  test "golden: createAuthor mutation operation is insert", %{parsed: parsed} do
    m = Enum.find(parsed["mutations"], &(&1["name"] == "createAuthor"))
    assert m["operation"] == "insert"
  end

  test "golden: createAuthor mutation has two arguments", %{parsed: parsed} do
    m = Enum.find(parsed["mutations"], &(&1["name"] == "createAuthor"))
    assert length(m["arguments"]) == 2
  end

  test "golden: bio argument in mutation is nullable", %{parsed: parsed} do
    m = Enum.find(parsed["mutations"], &(&1["name"] == "createAuthor"))
    bio_arg = Enum.find(m["arguments"], &(&1["name"] == "bio"))
    assert bio_arg["nullable"] == true
  end

  test "golden: name argument in mutation is not nullable", %{parsed: parsed} do
    m = Enum.find(parsed["mutations"], &(&1["name"] == "createAuthor"))
    name_arg = Enum.find(m["arguments"], &(&1["name"] == "name"))
    assert name_arg["nullable"] == false
  end

  test "golden: mutation name is camelCase", %{parsed: parsed} do
    names = Enum.map(parsed["mutations"], & &1["name"])
    assert "createAuthor" in names
  end

  test "golden: Author sql_source is v_author", %{parsed: parsed} do
    author = Enum.find(parsed["types"], &(&1["name"] == "Author"))
    assert author["sql_source"] == "v_author"
  end

  # ---------------------------------------------------------------------------
  # REST annotation tests
  # ---------------------------------------------------------------------------

  defmodule RestSchema do
    use FraiseQL.Schema

    fraiseql_type "User", sql_source: "v_user"

    fraiseql_query :users,
      return_type: "User",
      returns_list: true,
      sql_source: "v_user",
      rest_path: "/users",
      rest_method: "GET"

    fraiseql_query :user_no_rest,
      return_type: "User",
      sql_source: "v_user"

    fraiseql_mutation :create_user,
      return_type: "User",
      sql_source: "fn_create_user",
      operation: "insert",
      rest_path: "/users",
      rest_method: "POST"
  end

  test "query with rest_path emits rest block in JSON" do
    json = SchemaExporter.export(RestSchema)
    parsed = Jason.decode!(json)
    q = Enum.find(parsed["queries"], &(&1["name"] == "users"))
    assert q["rest"] == %{"path" => "/users", "method" => "GET"}
  end

  test "query without rest_path omits rest block in JSON" do
    json = SchemaExporter.export(RestSchema)
    parsed = Jason.decode!(json)
    q = Enum.find(parsed["queries"], &(&1["name"] == "user_no_rest"))
    refute Map.has_key?(q, "rest")
  end

  test "mutation with rest_path emits rest block in JSON" do
    json = SchemaExporter.export(RestSchema)
    parsed = Jason.decode!(json)
    m = Enum.find(parsed["mutations"], &(&1["name"] == "createUser"))
    assert m["rest"] == %{"path" => "/users", "method" => "POST"}
  end

  defmodule RestDefaultMethodSchema do
    use FraiseQL.Schema

    fraiseql_type "Item", sql_source: "v_item"

    fraiseql_query :items,
      return_type: "Item",
      returns_list: true,
      sql_source: "v_item",
      rest_path: "/items"

    fraiseql_mutation :create_item,
      return_type: "Item",
      sql_source: "fn_create_item",
      operation: "insert",
      rest_path: "/items"
  end

  test "query rest_path without rest_method defaults to GET" do
    json = SchemaExporter.export(RestDefaultMethodSchema)
    parsed = Jason.decode!(json)
    q = Enum.find(parsed["queries"], &(&1["name"] == "items"))
    assert q["rest"]["method"] == "GET"
  end

  test "mutation rest_path without rest_method defaults to POST" do
    json = SchemaExporter.export(RestDefaultMethodSchema)
    parsed = Jason.decode!(json)
    m = Enum.find(parsed["mutations"], &(&1["name"] == "createItem"))
    assert m["rest"]["method"] == "POST"
  end
end
