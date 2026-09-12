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

  # `fraiseql_query` builds its QueryDefinition in two places — one for the `do`-block
  # form and one for the form without — so a key added to one form and not the other is
  # carried by half the SDK's authoring surface and dropped by the other half, silently.
  # Both forms are exercised here for `pagination_order` (#1305).
  defmodule PaginationOrderSchema do
    use FraiseQL.Schema

    fraiseql_query :paged_no_block,
      return_type: "Author",
      returns_list: true,
      sql_source: "v_author",
      pagination_order: "created_at"

    fraiseql_query :paged_with_block,
      return_type: "Author",
      returns_list: true,
      sql_source: "v_author",
      pagination_order: "none" do
      argument :since, :string, nullable: true
    end

    fraiseql_query :paged_unset,
      return_type: "Author",
      returns_list: true,
      sql_source: "v_author"
  end

  # --- #1319: the same declaration authored both ways -------------------------
  #
  # `fraiseql_type`, `fraiseql_query` and `fraiseql_mutation` each have a `do`-block form
  # and a form without one, and each used to write its struct literal once per form. The
  # two copies differed in exactly one field — the argument/field buffer — so every new
  # authorable key had to be added twice, and adding it once left half the authoring
  # surface silently dropping it. Nothing about that is loud: the compiler derives a
  # default and the schema still compiles.
  #
  # Both forms now splice one struct. These fixtures author identical options both ways so
  # the tests below can assert the two results differ *only* in the buffer field. Every key
  # is set to a distinctive non-default value: two matching defaults would agree whether or
  # not the key survived the splice.

  defmodule TwoFormQuerySchema do
    use FraiseQL.Schema

    fraiseql_query :two_form_with_block,
      return_type: "Author",
      sql_source: "v_author",
      returns_list: true,
      nullable: true,
      cache_ttl_seconds: 300,
      description: "authored with a block",
      rest_path: "/authors",
      rest_method: "GET",
      inject_params: %{"tenant_id" => "jwt:tenant_id"},
      requires_role: "admin",
      requires_actor: ["human_user"],
      pagination_order: "created_at" do
      argument :id, :id, nullable: false
    end

    fraiseql_query :two_form_no_block,
      return_type: "Author",
      sql_source: "v_author",
      returns_list: true,
      nullable: true,
      cache_ttl_seconds: 300,
      description: "authored with a block",
      rest_path: "/authors",
      rest_method: "GET",
      inject_params: %{"tenant_id" => "jwt:tenant_id"},
      requires_role: "admin",
      requires_actor: ["human_user"],
      pagination_order: "created_at"
  end

  defmodule TwoFormMutationSchema do
    use FraiseQL.Schema

    fraiseql_mutation :two_form_with_block,
      return_type: "Author",
      sql_source: "fn_create_author",
      operation: "insert",
      description: "authored with a block",
      rest_path: "/authors",
      rest_method: "POST",
      inject_params: %{"tenant_id" => "jwt:tenant_id"},
      requires_role: "admin",
      requires_actor: ["service_account"],
      invalidates_views: ["v_author"],
      invalidates_fact_tables: ["tf_author"] do
      argument :name, :string, nullable: false
    end

    fraiseql_mutation :two_form_no_block,
      return_type: "Author",
      sql_source: "fn_create_author",
      operation: "insert",
      description: "authored with a block",
      rest_path: "/authors",
      rest_method: "POST",
      inject_params: %{"tenant_id" => "jwt:tenant_id"},
      requires_role: "admin",
      requires_actor: ["service_account"],
      invalidates_views: ["v_author"],
      invalidates_fact_tables: ["tf_author"]
  end

  defmodule TwoFormTypeSchema do
    use FraiseQL.Schema

    @relationship [
      [
        name: "orders",
        target_type: "Order",
        cardinality: "OneToMany",
        foreign_key: "fk_user",
        referenced_key: "id"
      ]
    ]

    fraiseql_type "TwoFormWithBlock",
      sql_source: "v_author",
      description: "authored with a block",
      relay: true,
      is_error: true,
      crud: true,
      cascade: true,
      relationships: @relationship do
      field :id, :id, nullable: false
    end

    # `crud: true` is absent here and only here: CRUD generation needs fields, so the
    # no-block form raises on it. That makes `crud` the one type key the two forms cannot
    # be compared on — it is asserted directly on the block form below, and the
    # cross-SDK `type_crud` construct covers the generated output.
    fraiseql_type "TwoFormNoBlock",
      sql_source: "v_author",
      description: "authored with a block",
      relay: true,
      is_error: true,
      cascade: true,
      relationships: @relationship

    # A second pair, because `is_input` cannot appear beside `sql_source` — an input
    # object has no backing relation and `__validate_type_opts__!` refuses the pair. It
    # is also this SDK's only route to an input object, so leaving it uncompared would
    # exempt the one key whose loss makes input types unauthorable.
    fraiseql_type "TwoFormInputWithBlock", is_input: true do
      field :id, :id, nullable: false
    end

    fraiseql_type "TwoFormInputNoBlock", is_input: true
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

  test "fraiseql_query carries pagination_order in both the block and no-block forms" do
    by_name =
      PaginationOrderSchema.__fraiseql_queries__()
      |> Map.new(&{&1.name, &1.pagination_order})

    assert by_name["pagedNoBlock"] == "created_at"
    assert by_name["pagedWithBlock"] == "none"
    # Unset stays nil rather than becoming "" or a guessed column: the compiler derives
    # the entity identity from an absent key, and a value invented here would override a
    # decision the author deliberately left to it.
    assert by_name["pagedUnset"] == nil
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
  # ---------------------------------------------------------------------------
  # Tests: the two macro forms agree on everything but the buffer (#1319)
  # ---------------------------------------------------------------------------

  # Keys the CRUD generator sets and the authoring macros deliberately never do:
  # `auto_params` is derived from a type's `crud:` (`crud_generator.ex`), and a
  # mutation's `cascade` from the type's own `cascade:` (`schema.ex`'s
  # `__before_compile__`). They are excluded from the two-form comparison because no
  # authoring path reaches them, not because they do not matter.
  @query_generator_only [:auto_params]
  @mutation_generator_only [:cascade]

  defp both_forms(definitions, buffer_key, generator_only) do
    by_name = Map.new(definitions, &{&1.name, &1})
    with_block = Map.fetch!(by_name, "twoFormWithBlock")
    no_block = Map.fetch!(by_name, "twoFormNoBlock")

    drop = [:name, buffer_key | generator_only]
    {with_block, no_block, Map.drop(Map.from_struct(with_block), drop),
     Map.drop(Map.from_struct(no_block), drop)}
  end

  test "fraiseql_query's two forms differ only in arguments" do
    {with_block, no_block, a, b} =
      both_forms(TwoFormQuerySchema.__fraiseql_queries__(), :arguments, @query_generator_only)

    # The one field that is *supposed* to differ, asserted so that a splice returning an
    # empty buffer for both forms cannot pass this test by making them trivially equal.
    assert length(with_block.arguments) == 1
    assert no_block.arguments == []

    assert a == b, """
    the block and no-block forms of fraiseql_query disagree on a key both should carry.
    A key added to one branch of the splice and not the other produces exactly this.
    block form: #{inspect(a)}
    no-block:   #{inspect(b)}
    """
  end

  test "fraiseql_mutation's two forms differ only in arguments" do
    {with_block, no_block, a, b} =
      both_forms(
        TwoFormMutationSchema.__fraiseql_mutations__(),
        :arguments,
        @mutation_generator_only
      )

    assert length(with_block.arguments) == 1
    assert no_block.arguments == []
    assert a == b
  end

  test "fraiseql_type's two forms differ only in fields" do
    by_name = Map.new(TwoFormTypeSchema.__fraiseql_types__(), &{&1.name, &1})

    # `crud` cannot be authored without a block at all — see the fixture — so it is
    # asserted here rather than compared.
    assert Map.fetch!(by_name, "TwoFormWithBlock").crud == true

    for prefix <- ["TwoForm", "TwoFormInput"] do
      with_block = Map.fetch!(by_name, prefix <> "WithBlock")
      no_block = Map.fetch!(by_name, prefix <> "NoBlock")

      assert length(with_block.fields) == 1
      assert no_block.fields == []

      drop = [:name, :fields, :crud]

      assert Map.drop(Map.from_struct(with_block), drop) ==
               Map.drop(Map.from_struct(no_block), drop),
             "the two forms of fraiseql_type disagree for #{prefix}"
    end
  end

  # The two-form tests above can only compare keys their fixtures set, so a key added to a
  # definition struct is invisible to them until someone remembers to author it. These
  # pins make forgetting loud: adding a key to one of the three structs fails here, naming
  # the fixture to extend. Without them the next key repeats #1319 with the tests green.

  test "every QueryDefinition key is covered by the two-form fixture" do
    assert_covered(
      %FraiseQL.QueryDefinition{name: "q", return_type: "T", sql_source: "v"},
      [:name, :arguments] ++ @query_generator_only,
      "TwoFormQuerySchema"
    )
  end

  test "every MutationDefinition key is covered by the two-form fixture" do
    assert_covered(
      %FraiseQL.MutationDefinition{
        name: "m",
        return_type: "T",
        sql_source: "fn_x",
        operation: "insert"
      },
      [:name, :arguments] ++ @mutation_generator_only,
      "TwoFormMutationSchema"
    )
  end

  test "every TypeDefinition key is covered by the two-form fixture" do
    assert_covered(
      %FraiseQL.TypeDefinition{name: "T", sql_source: "v"},
      [:name, :fields],
      "TwoFormTypeSchema"
    )
  end

  # A key is "covered" when the fixture authors it: the comparison then sees a real value
  # on both sides. Reading the fixture source rather than the compiled struct is
  # deliberate — the struct is what the splice produced, so asking it whether the fixture
  # set a key is asking the subject to vouch for its own test.
  defp assert_covered(empty_struct, exempt, fixture) do
    source = File.read!(__ENV__.file)

    [_, body] =
      Regex.run(~r/defmodule #{fixture} do\n(.*?)\n  end\n/s, source) ||
        flunk("could not find fixture #{fixture} in #{__ENV__.file}")

    keys = empty_struct |> Map.from_struct() |> Map.keys()

    missing =
      for key <- keys,
          key not in exempt,
          # The key may open its own line or sit inline after a comma; the lookbehind
          # keeps `input:` from matching inside `is_input:`.
          not Regex.match?(~r/(?<![a-z0-9_])#{key}:/, body),
          do: key

    assert missing == [], """
    #{fixture} does not author: #{inspect(missing)}.

    A key was added to #{inspect(empty_struct.__struct__)} without being set in both
    forms of #{fixture}, so the two-form test cannot see whether the splice carries it.
    Set it in both forms with a distinctive non-default value, or — if no authoring path
    reaches it — add it to the generator-only list with the producer named (#1319).
    """
  end
end
