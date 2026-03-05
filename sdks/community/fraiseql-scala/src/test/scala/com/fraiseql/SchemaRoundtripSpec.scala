package com.fraiseql

import com.fraiseql.schema.Schema
import org.scalatest.BeforeAndAfterEach
import org.scalatest.flatspec.AnyFlatSpec
import org.scalatest.matchers.should.Matchers

/**
 * SDK-3: Schema roundtrip golden test.
 *
 * Exercises the full decorator → JSON export pipeline: register a type
 * with fields (including a scoped field), export to JSON, and verify that
 * the output matches the expected schema.json structure exactly.
 *
 * This is the contract between the SDK and the fraiseql-cli compiler.
 * If the SDK produces malformed JSON the compiler rejects it — but
 * without this test that failure is silent during SDK development.
 */
class SchemaRoundtripSpec extends AnyFlatSpec with Matchers with BeforeAndAfterEach {

  override def beforeEach(): Unit = Schema.reset()
  override def afterEach(): Unit  = Schema.reset()

  "Schema roundtrip" should "produce the expected schema.json structure for a single type" in {
    // Register a realistic type with a mix of field types, including a scoped field.
    Schema.registerType("Article", Map(
      "id"    -> Map("type" -> "ID",     "nullable" -> false),
      "title" -> Map("type" -> "String", "nullable" -> false),
      "body"  -> Map("type" -> "String", "nullable" -> true),
      "email" -> Map("type" -> "String", "nullable" -> false, "scope" -> "read:Article.email"),
    ), Some("A published article"))

    val jsonStr = Schema.exportTypes(pretty = true)
    val parsed  = ujson.read(jsonStr)

    // Must contain exactly the `types` key — no compiler-reserved keys
    parsed.obj.contains("types")      shouldBe true
    parsed.obj.contains("queries")    shouldBe false
    parsed.obj.contains("mutations")  shouldBe false
    parsed.obj.contains("observers")  shouldBe false
    parsed.obj.contains("security")   shouldBe false
    parsed.obj.contains("federation") shouldBe false

    // Exactly one type was registered
    val types = parsed("types").arr
    types.length shouldBe 1

    // Verify Article type structure
    val article = types(0)
    article("name").str        shouldBe "Article"
    article("description").str shouldBe "A published article"

    // All four fields must be present
    val fieldNames = article("fields").arr.map(f => f("name").str)
    fieldNames should contain ("id")
    fieldNames should contain ("title")
    fieldNames should contain ("body")
    fieldNames should contain ("email")

    // The scoped field must carry its scope annotation
    val emailField = article("fields").arr.find(f => f("name").str == "email")
    emailField shouldBe defined
    emailField.get("scope").str shouldBe "read:Article.email"
  }

  it should "export multiple types with correct names" in {
    Schema.registerType("User",
      Map("id" -> Map("type" -> "ID", "nullable" -> false),
          "name" -> Map("type" -> "String", "nullable" -> false)),
      Some("System user"))

    Schema.registerType("Post",
      Map("id" -> Map("type" -> "ID", "nullable" -> false),
          "title" -> Map("type" -> "String", "nullable" -> false)),
      Some("Blog post"))

    val parsed = ujson.read(Schema.exportTypes(pretty = true))
    val types  = parsed("types").arr
    val names  = types.map(t => t("name").str).toSet

    types.length shouldBe 2
    names        should contain ("User")
    names        should contain ("Post")
  }

  it should "satisfy the schema.json structural contract" in {
    Schema.registerType("Order", Map(
      "id"     -> Map("type" -> "ID",     "nullable" -> false),
      "amount" -> Map("type" -> "Float",  "nullable" -> false),
      "status" -> Map("type" -> "String", "nullable" -> true),
    ))

    val parsed = ujson.read(Schema.exportTypes(pretty = true))

    // Top-level shape: only `types`
    parsed.obj.keySet shouldBe Set("types")

    // Every type entry must have `name` and `fields`
    for (t <- parsed("types").arr) {
      t.obj.contains("name")   shouldBe true
      t.obj.contains("fields") shouldBe true
      t("name").str              should not be empty
      t("fields").arr            should not be empty
    }
  }
}
