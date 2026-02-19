package com.fraiseql

import com.fraiseql.schema.Schema
import org.scalatest.BeforeAndAfterEach
import org.scalatest.flatspec.AnyFlatSpec
import org.scalatest.matchers.should.Matchers
import upickle.default.*
import java.nio.file.Files
import java.nio.file.Paths

/**
 * Tests for minimal types.json export (TOML-based workflow)
 *
 * Validates that Schema.exportTypes() function generates minimal schema
 * with only types (no queries, mutations, observers, security, etc.)
 */
class ExportTypesSpec extends AnyFlatSpec with Matchers with BeforeAndAfterEach {
  override def beforeEach(): Unit = {
    Schema.reset()
  }

  override def afterEach(): Unit = {
    Schema.reset()
  }

  "exportTypes" should "export minimal schema with single type" in {
    // Register a single type
    Schema.registerType("User", Map(
      "id" -> Map("type" -> "ID", "nullable" -> false),
      "name" -> Map("type" -> "String", "nullable" -> false),
      "email" -> Map("type" -> "String", "nullable" -> false),
    ), Some("User in the system"))

    // Export minimal types
    val json = Schema.exportTypes(pretty = true)
    val parsed = ujson.read(json)

    // Should have types section
    assert(parsed.obj.contains("types"))
    assert(parsed("types").isInstanceOf[ujson.Arr])
    assert(parsed("types").arr.length == 1)

    // Should NOT have queries, mutations, observers
    assert(!parsed.obj.contains("queries"))
    assert(!parsed.obj.contains("mutations"))
    assert(!parsed.obj.contains("observers"))
    assert(!parsed.obj.contains("authz_policies"))

    // Verify User type
    val userDef = parsed("types")(0)
    assert(userDef("name").str == "User")
    assert(userDef("description").str == "User in the system")
  }

  it should "export minimal schema with multiple types" in {
    // Register User type
    Schema.registerType("User", Map(
      "id" -> Map("type" -> "ID", "nullable" -> false),
      "name" -> Map("type" -> "String", "nullable" -> false),
    ))

    // Register Post type
    Schema.registerType("Post", Map(
      "id" -> Map("type" -> "ID", "nullable" -> false),
      "title" -> Map("type" -> "String", "nullable" -> false),
      "authorId" -> Map("type" -> "ID", "nullable" -> false),
    ))

    // Export minimal
    val json = Schema.exportTypes(pretty = true)
    val parsed = ujson.read(json)

    // Check types count
    assert(parsed("types").arr.length == 2)

    // Verify both types present
    val typeNames = parsed("types").arr.map(t => t("name").str)
    assert(typeNames.contains("User"))
    assert(typeNames.contains("Post"))
  }

  it should "not include queries in minimal export" in {
    // Register type
    Schema.registerType("User", Map(
      "id" -> Map("type" -> "ID", "nullable" -> false),
    ))

    // Export minimal
    val json = Schema.exportTypes(pretty = true)
    val parsed = ujson.read(json)

    // Should have types
    assert(parsed.obj.contains("types"))

    // Should NOT have queries
    assert(!parsed.obj.contains("queries"))
    assert(!parsed.obj.contains("mutations"))
  }

  it should "export compact format when pretty is false" in {
    Schema.registerType("User", Map(
      "id" -> Map("type" -> "ID", "nullable" -> false),
    ))

    // Export compact
    val compact = Schema.exportTypes(false)

    // Should be valid JSON
    val parsed = ujson.read(compact)
    assert(parsed.obj.contains("types"))

    // Compact JSON should be smaller or equal to pretty
    val pretty = Schema.exportTypes(true)
    assert(compact.length <= pretty.length)
  }

  it should "export pretty format when pretty is true" in {
    Schema.registerType("User", Map(
      "id" -> Map("type" -> "ID", "nullable" -> false),
    ))

    // Export pretty
    val json = Schema.exportTypes(true)

    // Should contain newlines (pretty format)
    assert(json.contains("\n"))

    // Should be valid JSON
    ujson.read(json)
  }

  it should "export types to file" in {
    Schema.registerType("User", Map(
      "id" -> Map("type" -> "ID", "nullable" -> false),
      "name" -> Map("type" -> "String", "nullable" -> false),
    ))

    // Export to temporary file
    val tmpFile = "/tmp/fraiseql_types_test_scala.json"

    // Remove file if exists
    if (Files.exists(Paths.get(tmpFile))) {
      Files.delete(Paths.get(tmpFile))
    }

    // Export to file
    Schema.exportTypesFile(tmpFile)

    // Verify file exists and is valid JSON
    assert(Files.exists(Paths.get(tmpFile)))

    val content = Files.readString(Paths.get(tmpFile))
    val parsed = ujson.read(content)

    assert(parsed.obj.contains("types"))
    assert(parsed("types").arr.length == 1)

    // Cleanup
    Files.delete(Paths.get(tmpFile))
  }

  it should "handle empty schema gracefully" in {
    // Export with no types registered
    val json = Schema.exportTypes(true)
    val parsed = ujson.read(json)

    // Should still have types key (as empty array)
    assert(parsed.obj.contains("types"))
    assert(parsed("types").isInstanceOf[ujson.Arr])
    assert(parsed("types").arr.isEmpty)
  }
}
