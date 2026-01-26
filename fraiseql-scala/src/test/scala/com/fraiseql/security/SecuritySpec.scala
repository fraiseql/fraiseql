package com.fraiseql.security

import org.scalatest.flatspec.AnyFlatSpec
import org.scalatest.matchers.should.Matchers

class AuthorizationSpec extends AnyFlatSpec with Matchers {
  "AuthorizeBuilder" should "create simple authorization rule" in {
    val config = new AuthorizeBuilder()
      .withRule("isOwner($context.userId, $field.ownerId)")
      .withDescription("Ownership check")
      .build()
    config.rule shouldEqual "isOwner($context.userId, $field.ownerId)"
    config.description shouldEqual "Ownership check"
  }

  it should "create authorization with policy" in {
    val config = new AuthorizeBuilder()
      .withPolicy("ownerOnly")
      .withDescription("References named policy")
      .build()
    config.policy shouldEqual "ownerOnly"
  }

  it should "support fluent chaining" in {
    val config = new AuthorizeBuilder()
      .withRule("hasPermission($context)")
      .withDescription("Complex rule")
      .withErrorMessage("Access denied")
      .withRecursive(true)
      .withOperations("read")
      .build()
    config.rule shouldEqual "hasPermission($context)"
    config.recursive shouldBe true
    config.operations shouldEqual "read"
  }

  it should "set caching configuration" in {
    val config = new AuthorizeBuilder()
      .withRule("checkAccess($context)")
      .withCacheable(true)
      .withCacheDurationSeconds(600)
      .build()
    config.cacheable shouldBe true
    config.cacheDurationSeconds shouldEqual 600
  }

  it should "set error message" in {
    val config = new AuthorizeBuilder()
      .withRule("adminOnly($context)")
      .withErrorMessage("Only administrators can access this")
      .build()
    config.errorMessage shouldEqual "Only administrators can access this"
  }

  it should "set recursive application" in {
    val config = new AuthorizeBuilder()
      .withRule("checkNested($context)")
      .withRecursive(true)
      .withDescription("Applied to nested types")
      .build()
    config.recursive shouldBe true
  }

  it should "set operation specific rule" in {
    val config = new AuthorizeBuilder()
      .withRule("canDelete($context)")
      .withOperations("delete")
      .withDescription("Only applies to delete operations")
      .build()
    config.operations shouldEqual "delete"
  }

  it should "convert to map" in {
    val config = new AuthorizeBuilder()
      .withRule("testRule")
      .withDescription("Test")
      .build()
    val map = config.toMap
    map("rule") shouldEqual "testRule"
    map("description") shouldEqual "Test"
  }

  it should "create multiple configurations" in {
    val config1 = new AuthorizeBuilder().withRule("rule1").build()
    val config2 = new AuthorizeBuilder().withRule("rule2").build()
    config1.rule should not equal config2.rule
  }

  it should "return default cache settings" in {
    val config = new AuthorizeBuilder().withRule("test").build()
    config.cacheable shouldBe true
    config.cacheDurationSeconds shouldEqual 300
  }

  it should "set all options" in {
    val config = new AuthorizeBuilder()
      .withRule("complex")
      .withPolicy("policy")
      .withDescription("Complex authorization")
      .withErrorMessage("Error")
      .withRecursive(true)
      .withOperations("create,read,update,delete")
      .withCacheable(false)
      .withCacheDurationSeconds(1000)
      .build()
    config.rule shouldEqual "complex"
    config.cacheable shouldBe false
    config.cacheDurationSeconds shouldEqual 1000
  }
}

class RoleBasedAccessControlSpec extends AnyFlatSpec with Matchers {
  "RoleRequiredBuilder" should "create single role requirement" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("admin"))
      .build()
    config.roles.length shouldEqual 1
    config.roles(0) shouldEqual "admin"
  }

  it should "create multiple role requirements" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("manager", "director"))
      .build()
    config.roles.length shouldEqual 2
    config.roles should contain("manager")
    config.roles should contain("director")
  }

  it should "use any role matching strategy" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("viewer", "editor"))
      .withStrategy(RoleMatchStrategy.Any)
      .withDescription("At least one role")
      .build()
    config.strategy shouldEqual RoleMatchStrategy.Any
  }

  it should "use all role matching strategy" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("admin", "auditor"))
      .withStrategy(RoleMatchStrategy.All)
      .withDescription("All roles required")
      .build()
    config.strategy shouldEqual RoleMatchStrategy.All
  }

  it should "use exactly role matching strategy" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("exact_role"))
      .withStrategy(RoleMatchStrategy.Exactly)
      .withDescription("Exactly these roles")
      .build()
    config.strategy shouldEqual RoleMatchStrategy.Exactly
  }

  it should "support role hierarchy" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("admin"))
      .withHierarchy(true)
      .withDescription("With hierarchy")
      .build()
    config.hierarchy shouldBe true
  }

  it should "support role inheritance" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("editor"))
      .withInherit(true)
      .withDescription("Inherits from parent")
      .build()
    config.inherit shouldBe true
  }

  it should "set operation specific roles" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("editor"))
      .withOperations("create,update")
      .withDescription("Only for edit operations")
      .build()
    config.operations shouldEqual "create,update"
  }

  it should "set custom error message" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("admin"))
      .withErrorMessage("Administrator access required")
      .build()
    config.errorMessage shouldEqual "Administrator access required"
  }

  it should "configure caching" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("viewer"))
      .withCacheable(true)
      .withCacheDurationSeconds(1800)
      .build()
    config.cacheable shouldBe true
    config.cacheDurationSeconds shouldEqual 1800
  }

  it should "create admin pattern" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("admin"))
      .withStrategy(RoleMatchStrategy.Any)
      .withDescription("Admin access")
      .build()
    config.roles.length shouldEqual 1
    config.roles(0) shouldEqual "admin"
  }

  it should "create manager director pattern" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("manager", "director"))
      .withStrategy(RoleMatchStrategy.Any)
      .withDescription("Managers and directors")
      .build()
    config.roles.length shouldEqual 2
    config.strategy shouldEqual RoleMatchStrategy.Any
  }

  it should "create data scientist pattern" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("data_scientist", "analyst"))
      .withStrategy(RoleMatchStrategy.Any)
      .withDescription("Data professionals")
      .build()
    config.roles.length shouldEqual 2
  }

  it should "convert to map" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("admin", "editor"))
      .withStrategy(RoleMatchStrategy.Any)
      .build()
    val map = config.toMap
    map("strategy") shouldEqual "any"
  }

  it should "set description" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("viewer"))
      .withDescription("Read-only access")
      .build()
    config.description shouldEqual "Read-only access"
  }

  it should "return default values" in {
    val config = new RoleRequiredBuilder()
      .withRoles(List("user"))
      .build()
    config.hierarchy shouldBe false
    config.inherit shouldBe false
    config.cacheable shouldBe true
    config.cacheDurationSeconds shouldEqual 300
  }
}

class AuthzPolicySpec extends AnyFlatSpec with Matchers {
  "AuthzPolicyBuilder" should "create RBAC policy" in {
    val config = new AuthzPolicyBuilder("adminOnly")
      .withType(AuthzPolicyType.Rbac)
      .withRule("hasRole($context, 'admin')")
      .withDescription("Access restricted to administrators")
      .withAuditLogging(true)
      .build()
    config.name shouldEqual "adminOnly"
    config.policyType shouldEqual AuthzPolicyType.Rbac
    config.rule shouldEqual "hasRole($context, 'admin')"
    config.auditLogging shouldBe true
  }

  it should "create ABAC policy" in {
    val config = new AuthzPolicyBuilder("secretClearance")
      .withType(AuthzPolicyType.Abac)
      .withDescription("Requires top secret clearance")
      .withAttributes(List("clearance_level >= 3", "background_check == true"))
      .build()
    config.name shouldEqual "secretClearance"
    config.policyType shouldEqual AuthzPolicyType.Abac
    config.attributes.length shouldEqual 2
  }

  it should "create custom policy" in {
    val config = new AuthzPolicyBuilder("customRule")
      .withType(AuthzPolicyType.Custom)
      .withRule("isOwner($context.userId, $resource.ownerId)")
      .withDescription("Custom ownership rule")
      .build()
    config.policyType shouldEqual AuthzPolicyType.Custom
  }

  it should "create hybrid policy" in {
    val config = new AuthzPolicyBuilder("auditAccess")
      .withType(AuthzPolicyType.Hybrid)
      .withDescription("Role and attribute-based access")
      .withRule("hasRole($context, 'auditor')")
      .withAttributes(List("audit_enabled == true"))
      .build()
    config.policyType shouldEqual AuthzPolicyType.Hybrid
    config.rule shouldEqual "hasRole($context, 'auditor')"
  }

  it should "create multiple policies" in {
    val policy1 = new AuthzPolicyBuilder("policy1").withType(AuthzPolicyType.Rbac).build()
    val policy2 = new AuthzPolicyBuilder("policy2").withType(AuthzPolicyType.Abac).build()
    val policy3 = new AuthzPolicyBuilder("policy3").withType(AuthzPolicyType.Custom).build()
    policy1.name shouldEqual "policy1"
    policy2.name shouldEqual "policy2"
    policy3.name shouldEqual "policy3"
  }

  it should "create PII access policy" in {
    val config = new AuthzPolicyBuilder("piiAccess")
      .withType(AuthzPolicyType.Rbac)
      .withDescription("Access to Personally Identifiable Information")
      .withRule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
      .build()
    config.name shouldEqual "piiAccess"
  }

  it should "create admin only policy" in {
    val config = new AuthzPolicyBuilder("adminOnly")
      .withType(AuthzPolicyType.Rbac)
      .withDescription("Admin-only access")
      .withRule("hasRole($context, 'admin')")
      .withAuditLogging(true)
      .build()
    config.auditLogging shouldBe true
  }

  it should "create recursive policy" in {
    val config = new AuthzPolicyBuilder("recursiveProtection")
      .withType(AuthzPolicyType.Custom)
      .withRule("canAccessNested($context)")
      .withRecursive(true)
      .withDescription("Recursively applies to nested types")
      .build()
    config.recursive shouldBe true
  }

  it should "create operation specific policy" in {
    val config = new AuthzPolicyBuilder("readOnly")
      .withType(AuthzPolicyType.Custom)
      .withRule("hasRole($context, 'viewer')")
      .withOperations("read")
      .withDescription("Policy applies only to read operations")
      .build()
    config.operations shouldEqual "read"
  }

  it should "create cached policy" in {
    val config = new AuthzPolicyBuilder("cachedAccess")
      .withType(AuthzPolicyType.Custom)
      .withRule("hasRole($context, 'viewer')")
      .withCacheable(true)
      .withCacheDurationSeconds(3600)
      .withDescription("Access control with result caching")
      .build()
    config.cacheable shouldBe true
    config.cacheDurationSeconds shouldEqual 3600
  }

  it should "create audited policy" in {
    val config = new AuthzPolicyBuilder("auditedAccess")
      .withType(AuthzPolicyType.Rbac)
      .withRule("hasRole($context, 'auditor')")
      .withAuditLogging(true)
      .withDescription("Access with comprehensive audit logging")
      .build()
    config.auditLogging shouldBe true
  }

  it should "create policy with error message" in {
    val config = new AuthzPolicyBuilder("restrictedAccess")
      .withType(AuthzPolicyType.Rbac)
      .withRule("hasRole($context, 'executive')")
      .withErrorMessage("Only executive level users can access this resource")
      .build()
    config.errorMessage shouldEqual "Only executive level users can access this resource"
  }

  it should "support fluent chaining" in {
    val config = new AuthzPolicyBuilder("complexPolicy")
      .withType(AuthzPolicyType.Hybrid)
      .withDescription("Complex hybrid policy")
      .withRule("hasRole($context, 'admin')")
      .withAttributes(List("security_clearance >= 3"))
      .withCacheable(true)
      .withCacheDurationSeconds(1800)
      .withRecursive(false)
      .withOperations("create,update,delete")
      .withAuditLogging(true)
      .withErrorMessage("Insufficient privileges")
      .build()
    config.name shouldEqual "complexPolicy"
    config.policyType shouldEqual AuthzPolicyType.Hybrid
    config.cacheable shouldBe true
    config.auditLogging shouldBe true
  }

  it should "create policy composition" in {
    val publicPolicy = new AuthzPolicyBuilder("publicAccess").withType(AuthzPolicyType.Rbac).withRule("true").build()
    val piiPolicy = new AuthzPolicyBuilder("piiAccess").withType(AuthzPolicyType.Rbac).withRule("hasRole($context, 'data_manager')").build()
    val adminPolicy = new AuthzPolicyBuilder("adminAccess").withType(AuthzPolicyType.Rbac).withRule("hasRole($context, 'admin')").build()
    publicPolicy.name shouldEqual "publicAccess"
    piiPolicy.name shouldEqual "piiAccess"
    adminPolicy.name shouldEqual "adminAccess"
  }

  it should "create financial data policy" in {
    val config = new AuthzPolicyBuilder("financialData")
      .withType(AuthzPolicyType.Abac)
      .withDescription("Access to financial records")
      .withAttributes(List("clearance_level >= 2", "department == \"finance\""))
      .build()
    config.name shouldEqual "financialData"
    config.attributes.length shouldEqual 2
  }

  it should "create security clearance policy" in {
    val config = new AuthzPolicyBuilder("secretClearance")
      .withType(AuthzPolicyType.Abac)
      .withAttributes(List("clearance_level >= 3", "background_check == true"))
      .withDescription("Requires top secret clearance")
      .build()
    config.attributes.length shouldEqual 2
  }

  it should "create default configuration" in {
    val config = new AuthzPolicyBuilder("default").build()
    config.name shouldEqual "default"
    config.policyType shouldEqual AuthzPolicyType.Custom
    config.cacheable shouldBe true
    config.cacheDurationSeconds shouldEqual 300
  }

  it should "convert to map" in {
    val config = new AuthzPolicyBuilder("test")
      .withType(AuthzPolicyType.Rbac)
      .withRule("test_rule")
      .build()
    val map = config.toMap
    map("name") shouldEqual "test"
    map("type") shouldEqual "rbac"
  }
}
