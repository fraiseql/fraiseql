defmodule FraiseQL.SecurityTest do
  use ExUnit.Case
  doctest FraiseQL.Security

  alias FraiseQL.Security

  # ===== Authorization Tests (11 tests) =====

  describe "authorize_config" do
    test "creates simple rule" do
      config = Security.authorize_config(rule: "test_rule")
      assert config.rule == "test_rule"
      assert config.policy == ""
      assert config.cacheable == true
    end

    test "uses policy reference" do
      config = Security.authorize_config(policy: "admin_policy")
      assert config.policy == "admin_policy"
      assert config.rule == ""
    end

    test "builder pattern with authorize/1" do
      builder = Security.authorize_builder()
      |> Map.put(:rule, "test_rule")
      |> Map.put(:description, "Test description")

      config = Security.authorize(builder)
      assert config.rule == "test_rule"
      assert config.description == "Test description"
    end

    test "caching configuration" do
      config = Security.authorize_config(
        rule: "test",
        cacheable: true,
        cache_duration_seconds: 600
      )
      assert config.cacheable == true
      assert config.cache_duration_seconds == 600
    end

    test "error message configuration" do
      config = Security.authorize_config(
        rule: "test",
        error_message: "Access denied"
      )
      assert config.error_message == "Access denied"
    end

    test "recursive application" do
      config = Security.authorize_config(
        rule: "test",
        recursive: true
      )
      assert config.recursive == true
    end

    test "operation-specific rules" do
      config = Security.authorize_config(
        rule: "test",
        operations: "read,write"
      )
      assert config.operations == "read,write"
    end

    test "defaults to cacheable=true and 300 seconds" do
      config = Security.authorize_config()
      assert config.cacheable == true
      assert config.cache_duration_seconds == 300
    end

    test "all configuration options" do
      config = Security.authorize_config(
        rule: "isOwner(...)",
        policy: "owner_policy",
        description: "Owner check",
        error_message: "Not owner",
        recursive: true,
        operations: "read",
        cacheable: false,
        cache_duration_seconds: 0
      )
      assert config.rule == "isOwner(...)"
      assert config.policy == "owner_policy"
      assert config.description == "Owner check"
      assert config.error_message == "Not owner"
      assert config.recursive == true
      assert config.operations == "read"
      assert config.cacheable == false
      assert config.cache_duration_seconds == 0
    end

    test "equality comparison" do
      config1 = Security.authorize_config(rule: "test")
      config2 = Security.authorize_config(rule: "test")
      assert config1 == config2
    end
  end

  # ===== Role-Based Access Control Tests (18 tests) =====

  describe "role_required_config" do
    test "single role requirement" do
      config = Security.role_required_config(roles: ["admin"])
      assert config.roles == ["admin"]
      assert config.strategy == :any
    end

    test "multiple roles" do
      config = Security.role_required_config(roles: ["manager", "director"])
      assert config.roles == ["manager", "director"]
    end

    test "role matching strategy: any" do
      config = Security.role_required_config(
        roles: ["admin", "manager"],
        strategy: :any
      )
      assert config.strategy == :any
      assert Security.strategy_to_string(:any) == "any"
    end

    test "role matching strategy: all" do
      config = Security.role_required_config(
        roles: ["admin", "auditor"],
        strategy: :all
      )
      assert config.strategy == :all
      assert Security.strategy_to_string(:all) == "all"
    end

    test "role matching strategy: exactly" do
      config = Security.role_required_config(
        roles: ["admin"],
        strategy: :exactly
      )
      assert config.strategy == :exactly
      assert Security.strategy_to_string(:exactly) == "exactly"
    end

    test "role hierarchy" do
      config = Security.role_required_config(
        roles: ["admin"],
        hierarchy: true
      )
      assert config.hierarchy == true
    end

    test "role inheritance" do
      config = Security.role_required_config(
        roles: ["user"],
        inherit: true
      )
      assert config.inherit == true
    end

    test "operation-specific requirements" do
      config = Security.role_required_config(
        roles: ["admin"],
        operations: "delete"
      )
      assert config.operations == "delete"
    end

    test "custom error message" do
      config = Security.role_required_config(
        roles: ["admin"],
        error_message: "Admin access required"
      )
      assert config.error_message == "Admin access required"
    end

    test "admin pattern" do
      config = Security.role_required_config(
        roles: ["admin"],
        description: "Admin only access"
      )
      assert config.roles == ["admin"]
      assert config.description == "Admin only access"
    end

    test "manager pattern" do
      config = Security.role_required_config(
        roles: ["manager", "director"],
        strategy: :any,
        description: "Manager or director"
      )
      assert config.roles == ["manager", "director"]
      assert config.strategy == :any
    end

    test "data scientist pattern" do
      config = Security.role_required_config(
        roles: ["data_scientist", "analyst"],
        strategy: :any
      )
      assert config.roles == ["data_scientist", "analyst"]
    end

    test "caching configuration" do
      config = Security.role_required_config(
        roles: ["admin"],
        cacheable: true,
        cache_duration_seconds: 1800
      )
      assert config.cacheable == true
      assert config.cache_duration_seconds == 1800
    end

    test "builder pattern with build_roles/1" do
      builder = Security.role_required_builder()
      |> Map.put(:roles, ["admin", "manager"])
      |> Map.put(:strategy, :any)

      config = Security.build_roles(builder)
      assert config.roles == ["admin", "manager"]
      assert config.strategy == :any
    end

    test "all configuration options" do
      config = Security.role_required_config(
        roles: ["admin", "manager"],
        strategy: :any,
        hierarchy: true,
        description: "Leadership",
        error_message: "Leader required",
        operations: "write",
        inherit: true,
        cacheable: false,
        cache_duration_seconds: 0
      )
      assert config.roles == ["admin", "manager"]
      assert config.strategy == :any
      assert config.hierarchy == true
      assert config.description == "Leadership"
      assert config.error_message == "Leader required"
      assert config.operations == "write"
      assert config.inherit == true
      assert config.cacheable == false
      assert config.cache_duration_seconds == 0
    end

    test "equality comparison" do
      config1 = Security.role_required_config(roles: ["admin"])
      config2 = Security.role_required_config(roles: ["admin"])
      assert config1 == config2
    end
  end

  # ===== Attribute-Based Access Control Tests (16 tests) =====

  describe "authz_policy_config with ABAC" do
    test "ABAC policy definition" do
      config = Security.authz_policy_config("secret_access",
        type: :abac,
        attributes: ["clearance_level >= 3"]
      )
      assert config.type == :abac
      assert config.attributes == ["clearance_level >= 3"]
    end

    test "multiple attributes" do
      config = Security.authz_policy_config("complex",
        type: :abac,
        attributes: ["department == 'engineering'", "tenure >= 2"]
      )
      assert Enum.count(config.attributes) == 2
    end

    test "clearance level checking" do
      config = Security.authz_policy_config("top_secret",
        type: :abac,
        attributes: ["clearance_level >= 5", "background_check == true"]
      )
      assert config.attributes == ["clearance_level >= 5", "background_check == true"]
    end

    test "department-based access" do
      config = Security.authz_policy_config("hr_only",
        type: :abac,
        attributes: ["department == 'human_resources'"]
      )
      assert config.attributes == ["department == 'human_resources'"]
    end

    test "time-based access control" do
      config = Security.authz_policy_config("business_hours",
        type: :abac,
        attributes: ["current_hour >= 9", "current_hour < 17"]
      )
      assert Enum.count(config.attributes) == 2
    end

    test "geographic restrictions" do
      config = Security.authz_policy_config("geo_limited",
        type: :abac,
        attributes: ["country == 'US'"]
      )
      assert config.attributes == ["country == 'US'"]
    end

    test "GDPR compliance" do
      config = Security.authz_policy_config("gdpr_compliant",
        type: :abac,
        attributes: ["region == 'EU'", "data_processing == true"]
      )
      assert config.attributes == ["region == 'EU'", "data_processing == true"]
    end

    test "data classification" do
      config = Security.authz_policy_config("pii_access",
        type: :abac,
        attributes: ["classification == 'public'"]
      )
      assert config.attributes == ["classification == 'public'"]
    end

    test "caching with TTL" do
      config = Security.authz_policy_config("cached_abac",
        type: :abac,
        attributes: ["test"],
        cacheable: true,
        cache_duration_seconds: 3600
      )
      assert config.cacheable == true
      assert config.cache_duration_seconds == 3600
    end

    test "audit logging" do
      config = Security.authz_policy_config("audited",
        type: :abac,
        attributes: ["test"],
        audit_logging: true
      )
      assert config.audit_logging == true
    end

    test "recursive attribute application" do
      config = Security.authz_policy_config("recursive_abac",
        type: :abac,
        attributes: ["test"],
        recursive: true
      )
      assert config.recursive == true
    end

    test "operation-specific attributes" do
      config = Security.authz_policy_config("write_restricted",
        type: :abac,
        attributes: ["test"],
        operations: "write"
      )
      assert config.operations == "write"
    end

    test "complex attribute combinations" do
      config = Security.authz_policy_config("complex_abac",
        type: :abac,
        attributes: ["a == 1", "b == 2", "c == 3"]
      )
      assert Enum.count(config.attributes) == 3
    end

    test "custom error message" do
      config = Security.authz_policy_config("error_abac",
        type: :abac,
        attributes: ["test"],
        error_message: "Insufficient attributes"
      )
      assert config.error_message == "Insufficient attributes"
    end

    test "equality comparison" do
      config1 = Security.authz_policy_config("test",
        type: :abac,
        attributes: ["a"]
      )
      config2 = Security.authz_policy_config("test",
        type: :abac,
        attributes: ["a"]
      )
      assert config1 == config2
    end
  end

  # ===== Authorization Policy Tests (19 tests) =====

  describe "authz_policy_config for all types" do
    test "RBAC policy type" do
      config = Security.authz_policy_config("admin_policy",
        type: :rbac,
        rule: "hasRole($context, 'admin')"
      )
      assert config.type == :rbac
      assert Security.policy_type_to_string(:rbac) == "rbac"
    end

    test "ABAC policy type" do
      config = Security.authz_policy_config("attribute_policy",
        type: :abac
      )
      assert config.type == :abac
      assert Security.policy_type_to_string(:abac) == "abac"
    end

    test "CUSTOM policy type" do
      config = Security.authz_policy_config("custom_policy",
        type: :custom,
        rule: "custom_expression"
      )
      assert config.type == :custom
      assert Security.policy_type_to_string(:custom) == "custom"
    end

    test "HYBRID policy type" do
      config = Security.authz_policy_config("hybrid_policy",
        type: :hybrid,
        rule: "hybrid_rule",
        attributes: ["attr1"]
      )
      assert config.type == :hybrid
      assert Security.policy_type_to_string(:hybrid) == "hybrid"
    end

    test "multiple policies" do
      config1 = Security.authz_policy_config("policy1")
      config2 = Security.authz_policy_config("policy2")
      assert config1.name == "policy1"
      assert config2.name == "policy2"
    end

    test "PII access policy" do
      config = Security.authz_policy_config("piiAccess",
        type: :rbac,
        rule: "hasRole($context, 'data_manager')",
        description: "PII data access"
      )
      assert config.name == "piiAccess"
      assert config.description == "PII data access"
    end

    test "admin-only policy" do
      config = Security.authz_policy_config("adminOnly",
        type: :rbac,
        rule: "hasRole($context, 'admin')"
      )
      assert config.name == "adminOnly"
      assert config.rule == "hasRole($context, 'admin')"
    end

    test "recursive policy application" do
      config = Security.authz_policy_config("recursive_policy",
        recursive: true
      )
      assert config.recursive == true
    end

    test "operation-specific policies" do
      config = Security.authz_policy_config("delete_policy",
        operations: "delete"
      )
      assert config.operations == "delete"
    end

    test "cached policies" do
      config = Security.authz_policy_config("cached_policy",
        cacheable: true,
        cache_duration_seconds: 3600
      )
      assert config.cacheable == true
      assert config.cache_duration_seconds == 3600
    end

    test "audited policies" do
      config = Security.authz_policy_config("audited_policy",
        audit_logging: true
      )
      assert config.audit_logging == true
    end

    test "custom error messages" do
      config = Security.authz_policy_config("error_policy",
        error_message: "Policy denied"
      )
      assert config.error_message == "Policy denied"
    end

    test "policy composition" do
      policy1 = Security.authz_policy_config("p1", type: :rbac)
      policy2 = Security.authz_policy_config("p2", type: :abac)
      assert policy1.type == :rbac
      assert policy2.type == :abac
    end

    test "builder pattern with build_policy/1" do
      builder = Security.authz_policy_builder("test_policy")
      |> Map.put(:type, :rbac)
      |> Map.put(:rule, "test_rule")

      config = Security.build_policy(builder)
      assert config.name == "test_policy"
      assert config.type == :rbac
      assert config.rule == "test_rule"
    end

    test "fluent builder chaining" do
      builder = Security.authz_policy_builder("chain_policy")
      |> Map.put(:type, :hybrid)
      |> Map.put(:rule, "rule1")
      |> Map.put(:attributes, ["attr1"])
      |> Map.put(:description, "Chained")

      config = Security.build_policy(builder)
      assert config.type == :hybrid
      assert config.rule == "rule1"
      assert config.attributes == ["attr1"]
      assert config.description == "Chained"
    end

    test "financial data policy" do
      config = Security.authz_policy_config("financialData",
        type: :rbac,
        rule: "hasRole($context, 'finance_manager')",
        audit_logging: true
      )
      assert config.name == "financialData"
      assert config.audit_logging == true
    end

    test "security clearance policy" do
      config = Security.authz_policy_config("securityClearance",
        type: :abac,
        attributes: ["clearance_level >= 3"]
      )
      assert config.name == "securityClearance"
      assert config.attributes == ["clearance_level >= 3"]
    end

    test "default configuration" do
      config = Security.authz_policy_config("default_policy")
      assert config.name == "default_policy"
      assert config.type == :custom
      assert config.description == ""
      assert config.rule == ""
      assert config.attributes == []
      assert config.cacheable == true
      assert config.cache_duration_seconds == 300
      assert config.recursive == false
      assert config.operations == ""
      assert config.audit_logging == false
      assert config.error_message == ""
    end

    test "equality comparison" do
      config1 = Security.authz_policy_config("test")
      config2 = Security.authz_policy_config("test")
      assert config1 == config2
    end
  end
end
