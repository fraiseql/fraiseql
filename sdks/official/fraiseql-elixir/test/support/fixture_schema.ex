defmodule FraiseQL.Test.FixtureSchema do
  @moduledoc false
  use FraiseQL.Schema

  fraiseql_type "Author", sql_source: "v_author", description: "A blog author" do
    field :id, :id, nullable: false
    field :name, :string, nullable: false
    field :bio, :string, nullable: true
    field :created_at, :datetime, nullable: false
  end

  fraiseql_type "Post", sql_source: "v_post" do
    field :id, :id, nullable: false
    field :title, :string, nullable: false
    field :author_id, :id, nullable: false
    field :body, :string, nullable: true
  end

  fraiseql_query :authors,
    return_type: "Author",
    returns_list: true,
    sql_source: "v_author"

  fraiseql_query :author, return_type: "Author", sql_source: "v_author" do
    argument :id, :id, nullable: false
  end

  fraiseql_mutation :create_author,
    return_type: "Author",
    sql_source: "fn_create_author",
    operation: "insert" do
    argument :name, :string, nullable: false
    argument :bio, :string, nullable: true
  end
end
