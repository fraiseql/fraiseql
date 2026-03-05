import XCTest
import Foundation
@testable import FraiseQL

/**
 SDK-3: Schema roundtrip golden test.

 Exercises the full decorator → JSON export pipeline: register a type
 with fields (including a scoped field), export to JSON, and verify that
 the output matches the expected schema.json structure exactly.

 This is the contract between the SDK and the fraiseql-cli compiler.
 If the SDK produces malformed JSON the compiler rejects it — but
 without this test that failure is silent during SDK development.
 */
final class SchemaRoundtripTests: XCTestCase {

    override func setUp() {
        super.setUp()
        Schema.reset()
    }

    override func tearDown() {
        super.tearDown()
        Schema.reset()
    }

    /// Full decorator → export pipeline produces the expected schema.json structure.
    func testRoundtripProducesExpectedSchemaStructure() throws {
        // Register a realistic type with a mix of field types, including a scoped field.
        Schema.registerType("Article", fields: [
            "id":    ["type": "ID",     "nullable": false],
            "title": ["type": "String", "nullable": false],
            "body":  ["type": "String", "nullable": true],
            "email": ["type": "String", "nullable": false, "scope": "read:Article.email"],
        ], description: "A published article")

        let json = Schema.exportTypes(pretty: true)
        let data = json.data(using: .utf8)!
        let parsed = try JSONSerialization.jsonObject(with: data) as! [String: Any]

        // Must contain exactly the `types` key — no compiler-reserved keys
        XCTAssertNotNil(parsed["types"],    "output must have `types` key")
        XCTAssertNil(parsed["queries"],     "output must NOT have `queries`")
        XCTAssertNil(parsed["mutations"],   "output must NOT have `mutations`")
        XCTAssertNil(parsed["observers"],   "output must NOT have `observers`")
        XCTAssertNil(parsed["security"],    "output must NOT have `security`")
        XCTAssertNil(parsed["federation"],  "output must NOT have `federation`")

        // Exactly one type was registered
        let types = parsed["types"] as! [[String: Any]]
        XCTAssertEqual(types.count, 1, "exactly one type was registered")

        // Verify Article type structure
        let article = types[0]
        XCTAssertEqual(article["name"] as? String, "Article",
                       "type name must be Article")
        XCTAssertEqual(article["description"] as? String, "A published article",
                       "description must round-trip")

        // All four fields must be present
        let fields     = article["fields"] as! [[String: Any]]
        let fieldNames = fields.compactMap { $0["name"] as? String }
        XCTAssertTrue(fieldNames.contains("id"),    "field `id` must be present")
        XCTAssertTrue(fieldNames.contains("title"), "field `title` must be present")
        XCTAssertTrue(fieldNames.contains("body"),  "field `body` must be present")
        XCTAssertTrue(fieldNames.contains("email"), "field `email` must be present")

        // The scoped field must carry its scope annotation
        let emailField = fields.first { $0["name"] as? String == "email" }
        XCTAssertNotNil(emailField, "email field must be present in output")
        XCTAssertEqual(emailField?["scope"] as? String, "read:Article.email",
                       "scope annotation must round-trip")
    }

    /// Multiple registered types all appear with correct names.
    func testRoundtripMultipleTypes() throws {
        Schema.registerType("User", fields: [
            "id":   ["type": "ID",     "nullable": false],
            "name": ["type": "String", "nullable": false],
        ], description: "System user")

        Schema.registerType("Post", fields: [
            "id":    ["type": "ID",     "nullable": false],
            "title": ["type": "String", "nullable": false],
        ], description: "Blog post")

        let json   = Schema.exportTypes(pretty: true)
        let data   = json.data(using: .utf8)!
        let parsed = try JSONSerialization.jsonObject(with: data) as! [String: Any]
        let types  = parsed["types"] as! [[String: Any]]
        let names  = Set(types.compactMap { $0["name"] as? String })

        XCTAssertEqual(types.count, 2, "two types must be exported")
        XCTAssertTrue(names.contains("User"), "User must be present")
        XCTAssertTrue(names.contains("Post"), "Post must be present")
    }

    /// Exported JSON satisfies the schema.json structural contract.
    func testRoundtripStructuralContract() throws {
        Schema.registerType("Order", fields: [
            "id":     ["type": "ID",     "nullable": false],
            "amount": ["type": "Float",  "nullable": false],
            "status": ["type": "String", "nullable": true],
        ])

        let json   = Schema.exportTypes(pretty: true)
        let data   = json.data(using: .utf8)!
        let parsed = try JSONSerialization.jsonObject(with: data) as! [String: Any]

        // Top-level shape: only `types`
        XCTAssertEqual(Set(parsed.keys), Set(["types"]),
                       "schema.json for types-only export must contain exactly the `types` key")

        // Every type entry must have `name` and `fields`
        let types = parsed["types"] as! [[String: Any]]
        for t in types {
            XCTAssertNotNil(t["name"],   "every type entry must have `name`")
            XCTAssertNotNil(t["fields"], "every type entry must have `fields`")
        }
    }
}
