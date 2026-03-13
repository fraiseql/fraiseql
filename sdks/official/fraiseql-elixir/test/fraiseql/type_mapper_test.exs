defmodule FraiseQL.TypeMapperTest do
  @moduledoc false
  use ExUnit.Case

  alias FraiseQL.TypeMapper

  # ---------------------------------------------------------------------------
  # to_graphql_type/1 — known atoms
  # ---------------------------------------------------------------------------

  test "maps :integer to Int" do
    assert TypeMapper.to_graphql_type(:integer) == "Int"
  end

  test "maps :int to Int" do
    assert TypeMapper.to_graphql_type(:int) == "Int"
  end

  test "maps :float to Float" do
    assert TypeMapper.to_graphql_type(:float) == "Float"
  end

  test "maps :boolean to Boolean" do
    assert TypeMapper.to_graphql_type(:boolean) == "Boolean"
  end

  test "maps :bool to Boolean" do
    assert TypeMapper.to_graphql_type(:bool) == "Boolean"
  end

  test "maps :string to String" do
    assert TypeMapper.to_graphql_type(:string) == "String"
  end

  test "maps :id to ID" do
    assert TypeMapper.to_graphql_type(:id) == "ID"
  end

  test "maps :datetime to DateTime" do
    assert TypeMapper.to_graphql_type(:datetime) == "DateTime"
  end

  # ---------------------------------------------------------------------------
  # to_graphql_type/1 — unknown atoms → PascalCase
  # ---------------------------------------------------------------------------

  test "maps :user_profile to UserProfile" do
    assert TypeMapper.to_graphql_type(:user_profile) == "UserProfile"
  end

  test "maps :blog_post_tag to BlogPostTag" do
    assert TypeMapper.to_graphql_type(:blog_post_tag) == "BlogPostTag"
  end

  test "maps :author to Author" do
    assert TypeMapper.to_graphql_type(:author) == "Author"
  end

  test "maps :order_item to OrderItem" do
    assert TypeMapper.to_graphql_type(:order_item) == "OrderItem"
  end

  test "maps :mutation_response to MutationResponse" do
    assert TypeMapper.to_graphql_type(:mutation_response) == "MutationResponse"
  end

  # ---------------------------------------------------------------------------
  # to_pascal_case/1
  # ---------------------------------------------------------------------------

  test "to_pascal_case converts single word" do
    assert TypeMapper.to_pascal_case(:author) == "Author"
  end

  test "to_pascal_case converts two words" do
    assert TypeMapper.to_pascal_case(:user_profile) == "UserProfile"
  end

  test "to_pascal_case converts three words" do
    assert TypeMapper.to_pascal_case(:blog_post_tag) == "BlogPostTag"
  end

  test "to_pascal_case handles already-pascal-style atom" do
    assert TypeMapper.to_pascal_case(:user) == "User"
  end

  # ---------------------------------------------------------------------------
  # to_camel_case/1
  # ---------------------------------------------------------------------------

  test "to_camel_case(:create_author) == createAuthor" do
    assert TypeMapper.to_camel_case(:create_author) == "createAuthor"
  end

  test "to_camel_case(:author) == author" do
    assert TypeMapper.to_camel_case(:author) == "author"
  end

  test "to_camel_case(:get_user_by_id) == getUserById" do
    assert TypeMapper.to_camel_case(:get_user_by_id) == "getUserById"
  end

  test "to_camel_case(:delete_post) == deletePost" do
    assert TypeMapper.to_camel_case(:delete_post) == "deletePost"
  end

  test "to_camel_case(:update_user_profile) == updateUserProfile" do
    assert TypeMapper.to_camel_case(:update_user_profile) == "updateUserProfile"
  end

  test "to_camel_case(:authors) == authors (no underscores)" do
    assert TypeMapper.to_camel_case(:authors) == "authors"
  end

  # ---------------------------------------------------------------------------
  # Struct key access (smoke-test for definitions)
  # ---------------------------------------------------------------------------

  test "FieldDefinition has correct keys" do
    f = %FraiseQL.FieldDefinition{name: "id", type: "ID", nullable: false}
    assert f.name == "id"
    assert f.type == "ID"
    assert f.nullable == false
    assert f.description == nil
    assert f.requires_scope == nil
    assert f.requires_scopes == nil
  end

  test "QueryDefinition nullable defaults to false" do
    q = %FraiseQL.QueryDefinition{name: "users", return_type: "User", sql_source: "v_user"}
    assert q.nullable == false
    assert q.returns_list == false
    assert q.arguments == []
  end

  test "IntermediateSchema version defaults to 2.0.0" do
    s = %FraiseQL.IntermediateSchema{}
    assert s.version == "2.0.0"
  end

  test "MutationDefinition enforce_keys prevents creation without required fields" do
    assert_raise ArgumentError, fn ->
      struct!(FraiseQL.MutationDefinition, name: "createFoo")
    end
  end
end
