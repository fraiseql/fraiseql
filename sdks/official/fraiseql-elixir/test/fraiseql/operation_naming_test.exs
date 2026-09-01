defmodule FraiseQL.OperationNamingTest do
  @moduledoc """
  An operation or argument declared the way an Elixir author writes an identifier — a
  snake_case atom — must reach the API camelCased, the same rule `field` has followed
  since #1249.

  `fraiseql_query` and `argument` published theirs verbatim through `Atom.to_string`, so
  `fraiseql_query :tenant_orders` put `tenant_orders` in the GraphQL API where the
  identical declaration in Python put `tenantOrders`. This SDK also disagreed with
  *itself*: `fraiseql_mutation` already camelCased its name and `fraiseql_query` did not.

  Nothing compared them, because the cross-SDK conformance fixture wrote every operation
  name as a camelCase atom — `:tenantOrders`, which no Elixir author would write — and
  every argument name in it was `:id`, `:email` or `:name`, one word each and therefore
  the same string in both conventions (#1255).
  """
  use ExUnit.Case

  defmodule NamingSchema do
    use FraiseQL.Schema

    fraiseql_type "Order", sql_source: "v_order" do
      field :id, :id, nullable: false
    end

    fraiseql_query :tenant_orders,
      return_type: "Order",
      returns_list: true,
      sql_source: "v_order" do
      argument(:include_archived, :boolean, nullable: true)
    end

    # One word: spells the same either way, so it would pass under either implementation.
    # Pinned so a later reader does not mistake it for coverage of the rule above.
    fraiseql_query(:users, return_type: "Order", returns_list: true, sql_source: "v_order")

    fraiseql_mutation :create_user,
      return_type: "Order",
      sql_source: "fn_create_user",
      operation: "insert" do
      argument(:display_name, :string, nullable: true)
    end
  end

  test "a query declared as a snake_case atom is published camelCase" do
    names = Enum.map(NamingSchema.__fraiseql_queries__(), & &1.name)
    assert "tenantOrders" in names
    refute "tenant_orders" in names
  end

  test "a mutation declared as a snake_case atom is published camelCase" do
    names = Enum.map(NamingSchema.__fraiseql_mutations__(), & &1.name)
    assert "createUser" in names
  end

  # Split from the mutation case below on purpose: this one has to find its query by
  # name, so it also fails when the *query-name* rule regresses. The mutation case does
  # not — `fraiseql_mutation` already camelCased its name before this change — so it
  # isolates the argument rule on its own.
  test "a query's argument names are published camelCase" do
    [query] = Enum.filter(NamingSchema.__fraiseql_queries__(), &(&1.name == "tenantOrders"))
    assert Enum.map(query.arguments, & &1.name) == ["includeArchived"]
  end

  test "a mutation's argument names are published camelCase" do
    [mutation] = Enum.filter(NamingSchema.__fraiseql_mutations__(), &(&1.name == "createUser"))
    assert Enum.map(mutation.arguments, & &1.name) == ["displayName"]
  end

  test "a one-word name is unchanged" do
    names = Enum.map(NamingSchema.__fraiseql_queries__(), & &1.name)
    assert "users" in names
  end
end
