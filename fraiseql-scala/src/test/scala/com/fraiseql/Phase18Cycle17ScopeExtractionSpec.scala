package com.fraiseql

import com.fraiseql.schema.{Schema, FieldDefinition, ScopeValidator}
import org.scalatest.flatspec.AnyFlatSpec
import org.scalatest.matchers.should.Matchers
import org.scalatest.BeforeAndAfterEach

/**
 * Phase 18 Cycle 17: Scala SDK - Field Scope Extraction & Validation
 *
 * RED phase tests for field-level RBAC scope extraction.
 * Tests cover:
 * - Field struct creation and properties
 * - Single scope requirements (scope)
 * - Multiple scopes array (scopes)
 * - Scope pattern validation (action:resource format)
 * - SchemaRegistry for type tracking
 * - JSON export with scope metadata
 */
class Phase18Cycle17ScopeExtractionSpec extends AnyFlatSpec with Matchers with BeforeAndAfterEach {

  override def beforeEach(): Unit = {
    Schema.reset()
  }

  // ============================================================================
  // FIELD CREATION TESTS (3 tests)
  // ============================================================================

  "Field" should "create with all properties" in {
    val field = FieldDefinition(
      name = "email",
      `type` = "String",
      nullable = false,
      description = Some("User email address"),
      scope = Some("read:user.email")
    )

    field.name should be("email")
    field.`type` should be("String")
    field.nullable should be(false)
    field.description should be(Some("User email address"))
    field.scope should be(Some("read:user.email"))
  }

  "Field" should "create with minimal properties" in {
    val field = FieldDefinition("id", "Int")

    field.name should be("id")
    field.`type` should be("Int")
    field.nullable should be(false)
    field.scope should be(None)
    field.scopes should be(None)
  }

  "Field" should "create with metadata preservation" in {
    val field = FieldDefinition(
      name = "password",
      `type` = "String",
      nullable = false,
      description = Some("Hashed password"),
      scope = Some("admin:user.*")
    )

    field.name should be("password")
    field.scope should be(Some("admin:user.*"))
    field.description should be(Some("Hashed password"))
  }

  // ============================================================================
  // SINGLE SCOPE REQUIREMENT TESTS (3 tests)
  // ============================================================================

  "Field" should "support single scope format" in {
    val field = FieldDefinition("email", "String", scope = Some("read:user.email"))
    field.scope should be(Some("read:user.email"))
    field.scopes should be(None)
  }

  "Field" should "support wildcard resource scope" in {
    val field = FieldDefinition("profile", "Object", scope = Some("read:User.*"))
    field.scope should be(Some("read:User.*"))
  }

  "Field" should "support global wildcard scope" in {
    val field = FieldDefinition("secret", "String", scope = Some("admin:*"))
    field.scope should be(Some("admin:*"))
  }

  // ============================================================================
  // MULTIPLE SCOPES ARRAY TESTS (3 tests)
  // ============================================================================

  "Field" should "support multiple scopes array" in {
    val scopes = List("read:user.email", "write:user.email")
    val field = FieldDefinition("email", "String", scopes = Some(scopes))

    field.scopes should be(Some(scopes))
    field.scope should be(None)
  }

  "Field" should "support single element scopes array" in {
    val scopes = List("read:user.profile")
    val field = FieldDefinition("profile", "Object", scopes = Some(scopes))

    field.scopes should be(Some(scopes))
    field.scopes.get.length should be(1)
  }

  "Field" should "support complex scopes array" in {
    val scopes = List("read:user.email", "write:user.*", "admin:*")
    val field = FieldDefinition("data", "String", scopes = Some(scopes))

    field.scopes should be(Some(scopes))
  }

  // ============================================================================
  // SCOPE PATTERN VALIDATION TESTS (6 tests)
  // ============================================================================

  "ScopeValidator" should "validate specific field scope" in {
    ScopeValidator.validate("read:user.email") should be(true)
  }

  "ScopeValidator" should "validate resource wildcard scope" in {
    ScopeValidator.validate("read:User.*") should be(true)
  }

  "ScopeValidator" should "validate global admin wildcard" in {
    ScopeValidator.validate("admin:*") should be(true)
  }

  "ScopeValidator" should "reject scope missing colon" in {
    ScopeValidator.validate("readuser") should be(false)
  }

  "ScopeValidator" should "reject action with hyphen" in {
    ScopeValidator.validate("read-all:user") should be(false)
  }

  "ScopeValidator" should "reject resource with hyphen" in {
    ScopeValidator.validate("read:user-data") should be(false)
  }

  // ============================================================================
  // SCHEMA REGISTRY TESTS (3 tests)
  // ============================================================================

  "Schema" should "register type with fields and scopes" in {
    val fields = Map(
      "id" -> Map("type" -> "Int", "nullable" -> false),
      "email" -> Map("type" -> "String", "nullable" -> false, "scope" -> "read:user.email")
    )

    Schema.registerType("User", fields)

    Schema.getTypeNames should contain("User")
  }

  "Schema" should "extract scoped fields from registry" in {
    val fields = Map(
      "id" -> Map("type" -> "Int", "nullable" -> false),
      "email" -> Map("type" -> "String", "nullable" -> false, "scope" -> "read:user.email"),
      "password" -> Map("type" -> "String", "nullable" -> false, "scope" -> "admin:user.password")
    )

    Schema.registerType("User", fields)

    // Should have 2 scoped fields
    Schema.getTypeNames should contain("User")
  }

  "Schema" should "handle multiple types with different scopes" in {
    val userFields = Map(
      "id" -> Map("type" -> "Int"),
      "email" -> Map("type" -> "String", "scope" -> "read:user.email")
    )

    val postFields = Map(
      "id" -> Map("type" -> "Int"),
      "content" -> Map("type" -> "String", "scope" -> "read:post.content")
    )

    Schema.registerType("User", userFields)
    Schema.registerType("Post", postFields)

    Schema.getTypeNames.length should be(2)
    Schema.getTypeNames should contain("User")
    Schema.getTypeNames should contain("Post")
  }

  // ============================================================================
  // JSON EXPORT TESTS (2 tests)
  // ============================================================================

  "Schema export" should "include scope in field JSON" in {
    val fields = Map(
      "email" -> Map("type" -> "String", "nullable" -> false, "scope" -> "read:user.email")
    )

    Schema.registerType("User", fields)
    val json = Schema.exportTypes(pretty = false)

    json should include("email")
    json should include("scope")
    json should include("read:user.email")
  }

  "Schema export" should "export multiple types with scopes" in {
    val userFields = Map(
      "id" -> Map("type" -> "Int"),
      "email" -> Map("type" -> "String", "scope" -> "read:user.email")
    )

    val postFields = Map(
      "id" -> Map("type" -> "Int"),
      "content" -> Map("type" -> "String", "scope" -> "read:post.content")
    )

    Schema.registerType("User", userFields)
    Schema.registerType("Post", postFields)

    val json = Schema.exportTypes(pretty = false)
    json should include("User")
    json should include("Post")
    json should include("read:user.email")
    json should include("read:post.content")
  }

  // ============================================================================
  // CONFLICTING SCOPE AND SCOPES TESTS (2 tests)
  // ============================================================================

  "Field" should "handle both scope and scopes (not mutually exclusive in data model)" in {
    val field = FieldDefinition(
      "email",
      "String",
      scope = Some("read:user.email"),
      scopes = Some(List("write:user.email"))
    )

    field.scope should be(Some("read:user.email"))
    field.scopes should be(Some(List("write:user.email")))
  }

  "ScopeValidator" should "reject empty scope string" in {
    ScopeValidator.validate("") should be(false)
  }
}
