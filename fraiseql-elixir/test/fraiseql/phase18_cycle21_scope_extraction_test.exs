defmodule FraiseQL.Phase18Cycle21ScopeExtractionTest do
  use ExUnit.Case
  doctest FraiseQL.Schema

  setup do
    FraiseQL.Schema.start_link([])
    :ok
  end

  # MARK: - Field Creation Tests (3 tests)

  test "field should create with all properties" do
    fields = %{
      "email" => %{
        type: "String",
        nullable: false,
        description: "User email address",
        requires_scope: "read:user.email"
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    assert type_info != nil
    {field_config, _} = type_info

    email_field = Map.get(field_config, "email")
    assert Map.get(email_field, :type) == "String"
    assert Map.get(email_field, :nullable) == false
    assert Map.get(email_field, :description) == "User email address"
    assert Map.get(email_field, :requires_scope) == "read:user.email"

    FraiseQL.Schema.reset()
  end

  test "field should create with minimal properties" do
    fields = %{"id" => %{type: "Int"}}

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    assert type_info != nil
    {field_config, _} = type_info

    id_field = Map.get(field_config, "id")
    assert Map.get(id_field, :type) == "Int"
    assert Map.get(id_field, :requires_scope) == nil
    assert Map.get(id_field, :requires_scopes) == nil

    FraiseQL.Schema.reset()
  end

  test "field should preserve metadata alongside scopes" do
    fields = %{
      "password" => %{
        type: "String",
        nullable: false,
        description: "Hashed password",
        requires_scope: "admin:user.*"
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    assert type_info != nil
    {field_config, _} = type_info

    password_field = Map.get(field_config, "password")
    assert Map.get(password_field, :type) == "String"
    assert Map.get(password_field, :requires_scope) == "admin:user.*"
    assert Map.get(password_field, :description) == "Hashed password"

    FraiseQL.Schema.reset()
  end

  # MARK: - Single Scope Requirement Tests (3 tests)

  test "field should support single scope format" do
    fields = %{
      "email" => %{
        type: "String",
        requires_scope: "read:user.email"
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    {field_config, _} = type_info

    email_field = Map.get(field_config, "email")
    assert Map.get(email_field, :requires_scope) == "read:user.email"
    assert Map.get(email_field, :requires_scopes) == nil

    FraiseQL.Schema.reset()
  end

  test "field should support wildcard resource scope" do
    fields = %{
      "profile" => %{
        type: "Object",
        requires_scope: "read:User.*"
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    {field_config, _} = type_info

    profile_field = Map.get(field_config, "profile")
    assert Map.get(profile_field, :requires_scope) == "read:User.*"

    FraiseQL.Schema.reset()
  end

  test "field should support global wildcard scope" do
    fields = %{
      "secret" => %{
        type: "String",
        requires_scope: "admin:*"
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    {field_config, _} = type_info

    secret_field = Map.get(field_config, "secret")
    assert Map.get(secret_field, :requires_scope) == "admin:*"

    FraiseQL.Schema.reset()
  end

  # MARK: - Multiple Scopes Array Tests (3 tests)

  test "field should support multiple scopes array" do
    fields = %{
      "email" => %{
        type: "String",
        requires_scopes: ["read:user.email", "write:user.email"]
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    {field_config, _} = type_info

    email_field = Map.get(field_config, "email")
    scopes = Map.get(email_field, :requires_scopes)
    assert scopes != nil
    assert length(scopes) == 2
    assert "read:user.email" in scopes
    assert "write:user.email" in scopes

    FraiseQL.Schema.reset()
  end

  test "field should support single element scopes array" do
    fields = %{
      "profile" => %{
        type: "Object",
        requires_scopes: ["read:user.profile"]
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    {field_config, _} = type_info

    profile_field = Map.get(field_config, "profile")
    scopes = Map.get(profile_field, :requires_scopes)
    assert scopes != nil
    assert length(scopes) == 1
    assert Enum.at(scopes, 0) == "read:user.profile"

    FraiseQL.Schema.reset()
  end

  test "field should support complex scopes array" do
    fields = %{
      "data" => %{
        type: "String",
        requires_scopes: ["read:user.email", "write:user.*", "admin:*"]
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    {field_config, _} = type_info

    data_field = Map.get(field_config, "data")
    scopes = Map.get(data_field, :requires_scopes)
    assert scopes != nil
    assert length(scopes) == 3

    FraiseQL.Schema.reset()
  end

  # MARK: - Scope Pattern Validation Tests (6 tests)

  test "scope validator should validate specific field scope" do
    fields = %{
      "email" => %{
        type: "String",
        requires_scope: "read:user.email"
      }
    }

    assert_no_raise(fn -> FraiseQL.Schema.register_type("User", fields) end)
    FraiseQL.Schema.reset()
  end

  test "scope validator should validate resource wildcard scope" do
    fields = %{
      "profile" => %{
        type: "Object",
        requires_scope: "read:User.*"
      }
    }

    assert_no_raise(fn -> FraiseQL.Schema.register_type("User", fields) end)
    FraiseQL.Schema.reset()
  end

  test "scope validator should validate global admin wildcard" do
    fields = %{
      "secret" => %{
        type: "String",
        requires_scope: "admin:*"
      }
    }

    assert_no_raise(fn -> FraiseQL.Schema.register_type("User", fields) end)
    FraiseQL.Schema.reset()
  end

  test "scope validator should reject scope missing colon" do
    fields = %{
      "data" => %{
        type: "String",
        requires_scope: "readuser"
      }
    }

    assert_raise FraiseQL.ScopeValidationError, fn ->
      FraiseQL.Schema.register_type("User", fields)
    end

    FraiseQL.Schema.reset()
  end

  test "scope validator should reject action with hyphen" do
    fields = %{
      "data" => %{
        type: "String",
        requires_scope: "read-all:user"
      }
    }

    assert_raise FraiseQL.ScopeValidationError, fn ->
      FraiseQL.Schema.register_type("User", fields)
    end

    FraiseQL.Schema.reset()
  end

  test "scope validator should reject resource with hyphen" do
    fields = %{
      "data" => %{
        type: "String",
        requires_scope: "read:user-data"
      }
    }

    assert_raise FraiseQL.ScopeValidationError, fn ->
      FraiseQL.Schema.register_type("User", fields)
    end

    FraiseQL.Schema.reset()
  end

  # MARK: - Schema Registry Tests (3 tests)

  test "schema should register type with fields and scopes" do
    fields = %{
      "id" => %{type: "Int", nullable: false},
      "email" => %{
        type: "String",
        nullable: false,
        requires_scope: "read:user.email"
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_names = FraiseQL.Schema.get_type_names()
    assert "User" in type_names

    FraiseQL.Schema.reset()
  end

  test "schema should extract scoped fields from registry" do
    fields = %{
      "id" => %{type: "Int", nullable: false},
      "email" => %{
        type: "String",
        nullable: false,
        requires_scope: "read:user.email"
      },
      "password" => %{
        type: "String",
        nullable: false,
        requires_scope: "admin:user.password"
      }
    }

    FraiseQL.Schema.register_type("User", fields)

    type_info = FraiseQL.Schema.get_type("User")
    assert type_info != nil
    {field_config, _} = type_info

    email_field = Map.get(field_config, "email")
    assert Map.get(email_field, :requires_scope) == "read:user.email"

    password_field = Map.get(field_config, "password")
    assert Map.get(password_field, :requires_scope) == "admin:user.password"

    FraiseQL.Schema.reset()
  end

  test "schema should handle multiple types with different scopes" do
    FraiseQL.Schema.register_type("User", %{
      "id" => %{type: "Int"},
      "email" => %{
        type: "String",
        requires_scope: "read:user.email"
      }
    })

    FraiseQL.Schema.register_type("Post", %{
      "id" => %{type: "Int"},
      "content" => %{
        type: "String",
        requires_scope: "read:post.content"
      }
    })

    type_names = FraiseQL.Schema.get_type_names()
    assert length(type_names) == 2
    assert "User" in type_names
    assert "Post" in type_names

    FraiseQL.Schema.reset()
  end

  # MARK: - JSON Export Tests (2 tests)

  test "schema export should include scope in field JSON" do
    fields = %{
      "email" => %{
        type: "String",
        nullable: false,
        requires_scope: "read:user.email"
      }
    }

    FraiseQL.Schema.register_type("User", fields)
    json = FraiseQL.Schema.export_types(false)

    assert String.contains?(json, "User")
    assert String.contains?(json, "email")
    assert String.contains?(json, "read:user.email")
    assert String.contains?(json, "requires_scope")

    FraiseQL.Schema.reset()
  end

  test "schema export should export multiple types with scopes" do
    FraiseQL.Schema.register_type("User", %{
      "id" => %{type: "Int"},
      "email" => %{
        type: "String",
        requires_scope: "read:user.email"
      }
    })

    FraiseQL.Schema.register_type("Post", %{
      "id" => %{type: "Int"},
      "content" => %{
        type: "String",
        requires_scope: "read:post.content"
      }
    })

    json = FraiseQL.Schema.export_types(false)

    assert String.contains?(json, "User")
    assert String.contains?(json, "Post")
    assert String.contains?(json, "read:user.email")
    assert String.contains?(json, "read:post.content")

    FraiseQL.Schema.reset()
  end

  # MARK: - Conflicting Scope/Scopes Tests (2 tests)

  test "field with both scope and scopes should be rejected" do
    fields = %{
      "email" => %{
        type: "String",
        requires_scope: "read:user.email",
        requires_scopes: ["write:user.email"]
      }
    }

    assert_raise FraiseQL.ScopeValidationError, fn ->
      FraiseQL.Schema.register_type("User", fields)
    end

    FraiseQL.Schema.reset()
  end

  test "scope validator should reject empty scope string" do
    fields = %{
      "data" => %{
        type: "String",
        requires_scope: ""
      }
    }

    assert_raise FraiseQL.ScopeValidationError, fn ->
      FraiseQL.Schema.register_type("User", fields)
    end

    FraiseQL.Schema.reset()
  end

  defp assert_no_raise(fun) do
    try do
      fun.()
      assert true
    rescue
      e ->
        flunk("Expected no error but got: #{inspect(e)}")
    end
  end
end
