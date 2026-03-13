defmodule FraiseQL.SchemaDslTest do
  @moduledoc false
  use ExUnit.Case

  # ---------------------------------------------------------------------------
  # Test fixture modules — defined at top level so they compile once
  # ---------------------------------------------------------------------------

  defmodule EmptySchema do
    use FraiseQL.Schema
  end

  defmodule AuthorSchema do
    use FraiseQL.Schema

    fraiseql_type "Author", sql_source: "v_author", description: "A blog author" do
      field :id, :id, nullable: false
      field :name, :string, nullable: false
      field :bio, :string, nullable: true
    end
  end

  defmodule MultiTypeSchema do
    use FraiseQL.Schema

    fraiseql_type "Author", sql_source: "v_author" do
      field :id, :id, nullable: false
      field :name, :string, nullable: false
    end

    fraiseql_type "Post", sql_source: "v_post" do
      field :id, :id, nullable: false
      field :title, :string, nullable: false
    end
  end

  defmodule QuerySchema do
    use FraiseQL.Schema

    fraiseql_query :authors,
      return_type: "Author",
      returns_list: true,
      sql_source: "v_author"

    fraiseql_query :author, return_type: "Author", sql_source: "v_author" do
      argument :id, :id, nullable: false
    end
  end

  defmodule MutationSchema do
    use FraiseQL.Schema

    fraiseql_mutation :create_author,
      return_type: "Author",
      sql_source: "fn_create_author",
      operation: "insert" do
      argument :name, :string, nullable: false
    end
  end

  defmodule ScopedFieldSchema do
    use FraiseQL.Schema

    fraiseql_type "User", sql_source: "v_user" do
      field :id, :id, nullable: false
      field :email, :string, nullable: false, requires_scope: "read:user.email"
      field :roles, :string, nullable: true, requires_scopes: ["admin:read", "read:roles"]
    end
  end

  defmodule NoArgMutationSchema do
    use FraiseQL.Schema

    fraiseql_mutation :delete_author,
      return_type: "Author",
      sql_source: "fn_delete_author",
      operation: "delete"
  end

  # ---------------------------------------------------------------------------
  # Tests: bare use compiles
  # ---------------------------------------------------------------------------

  test "a module that uses FraiseQL.Schema compiles" do
    assert Code.ensure_loaded?(EmptySchema)
  end

  test "empty schema has zero types, queries, mutations" do
    assert EmptySchema.__fraiseql_types__() == []
    assert EmptySchema.__fraiseql_queries__() == []
    assert EmptySchema.__fraiseql_mutations__() == []
  end

  # ---------------------------------------------------------------------------
  # Tests: fraiseql_type
  # ---------------------------------------------------------------------------

  test "fraiseql_type registers a type with fields" do
    types = AuthorSchema.__fraiseql_types__()
    assert length(types) == 1
    [author] = types
    assert author.name == "Author"
    assert author.sql_source == "v_author"
    assert author.description == "A blog author"
    assert length(author.fields) == 3
  end

  test "fraiseql_type field names are strings" do
    [author] = AuthorSchema.__fraiseql_types__()
    id_field = Enum.find(author.fields, &(&1.name == "id"))
    assert id_field != nil
    assert id_field.type == "ID"
    assert id_field.nullable == false
  end

  test "fraiseql_type nullable field" do
    [author] = AuthorSchema.__fraiseql_types__()
    bio_field = Enum.find(author.fields, &(&1.name == "bio"))
    assert bio_field.nullable == true
  end

  test "fraiseql_type field order is preserved" do
    [author] = AuthorSchema.__fraiseql_types__()
    names = Enum.map(author.fields, & &1.name)
    assert names == ["id", "name", "bio"]
  end

  test "multiple fraiseql_type declarations accumulate" do
    types = MultiTypeSchema.__fraiseql_types__()
    assert length(types) == 2
    names = Enum.map(types, & &1.name)
    assert "Author" in names
    assert "Post" in names
  end

  test "multiple type declaration order is preserved" do
    [first, second] = MultiTypeSchema.__fraiseql_types__()
    assert first.name == "Author"
    assert second.name == "Post"
  end

  test "field with requires_scope is preserved" do
    [user] = ScopedFieldSchema.__fraiseql_types__()
    email_field = Enum.find(user.fields, &(&1.name == "email"))
    assert email_field.requires_scope == "read:user.email"
    assert email_field.requires_scopes == nil
  end

  test "field with requires_scopes is preserved" do
    [user] = ScopedFieldSchema.__fraiseql_types__()
    roles_field = Enum.find(user.fields, &(&1.name == "roles"))
    assert roles_field.requires_scopes == ["admin:read", "read:roles"]
    assert roles_field.requires_scope == nil
  end

  # ---------------------------------------------------------------------------
  # Tests: fraiseql_query
  # ---------------------------------------------------------------------------

  test "fraiseql_query without block registers a query with no arguments" do
    queries = QuerySchema.__fraiseql_queries__()
    [q] = Enum.filter(queries, &(&1.name == "authors"))
    assert q.return_type == "Author"
    assert q.returns_list == true
    assert q.nullable == false
    assert q.sql_source == "v_author"
    assert q.arguments == []
  end

  test "fraiseql_query with block registers arguments" do
    queries = QuerySchema.__fraiseql_queries__()
    [q] = Enum.filter(queries, &(&1.name == "author"))
    assert length(q.arguments) == 1
    [arg] = q.arguments
    assert arg.name == "id"
    assert arg.type == "ID"
    assert arg.nullable == false
  end

  test "fraiseql_query query name atom is converted to string" do
    queries = QuerySchema.__fraiseql_queries__()
    names = Enum.map(queries, & &1.name)
    assert "authors" in names
    assert "author" in names
  end

  # ---------------------------------------------------------------------------
  # Tests: fraiseql_mutation
  # ---------------------------------------------------------------------------

  test "fraiseql_mutation registers a mutation with arguments" do
    mutations = MutationSchema.__fraiseql_mutations__()
    [m] = mutations
    assert m.name == "createAuthor"
    assert m.sql_source == "fn_create_author"
    assert m.operation == "insert"
    assert m.return_type == "Author"
    assert length(m.arguments) == 1
  end

  test "fraiseql_mutation name atom is converted to camelCase" do
    [m] = MutationSchema.__fraiseql_mutations__()
    assert m.name == "createAuthor"
  end

  test "fraiseql_mutation without block has empty arguments" do
    [m] = NoArgMutationSchema.__fraiseql_mutations__()
    assert m.name == "deleteAuthor"
    assert m.arguments == []
  end

  # ---------------------------------------------------------------------------
  # Tests: @before_compile helper functions
  # ---------------------------------------------------------------------------

  test "schema module has export_to_file!/1 helper" do
    assert function_exported?(AuthorSchema, :export_to_file!, 1)
  end

  test "schema module has export_to_file!/2 helper" do
    assert function_exported?(AuthorSchema, :export_to_file!, 2)
  end

  test "schema module has to_intermediate_schema/0 helper" do
    assert function_exported?(AuthorSchema, :to_intermediate_schema, 0)
  end

  # ---------------------------------------------------------------------------
  # Tests: compile-time validations
  # ---------------------------------------------------------------------------

  test "raises ArgumentError when sql_source is missing from fraiseql_type" do
    assert_raise ArgumentError, ~r/sql_source is required/, fn ->
      Code.compile_string("""
      defmodule FraiseQL.SchemaDslTest.BadType do
        use FraiseQL.Schema
        fraiseql_type "Bad", description: "no source" do
          field :id, :id, nullable: false
        end
      end
      """)
    end
  end

  test "raises ArgumentError on duplicate type names" do
    assert_raise ArgumentError, ~r/duplicate type name.*Author/, fn ->
      Code.compile_string("""
      defmodule FraiseQL.SchemaDslTest.DuplicateType do
        use FraiseQL.Schema
        fraiseql_type "Author", sql_source: "v_author" do
          field :id, :id, nullable: false
        end
        fraiseql_type "Author", sql_source: "v_author2" do
          field :id, :id, nullable: false
        end
      end
      """)
    end
  end
end
