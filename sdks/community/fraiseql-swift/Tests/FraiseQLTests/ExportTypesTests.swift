import XCTest
import Foundation
@testable import FraiseQL

final class ExportTypesTests: XCTestCase {
  override func setUp() {
    super.setUp()
    Schema.reset()
  }

  override func tearDown() {
    super.tearDown()
    Schema.reset()
  }

  func testExportTypesMinimalSingleType() throws {
    Schema.registerType("User", fields: [
      "id": ["type": "ID", "nullable": false],
      "name": ["type": "String", "nullable": false],
      "email": ["type": "String", "nullable": false],
    ], description: "User in the system")

    let json = Schema.exportTypes(pretty: true)
    let data = json.data(using: .utf8)!
    let parsed = try JSONSerialization.jsonObject(with: data) as! [String: Any]

    XCTAssertNotNil(parsed["types"])
    let types = parsed["types"] as! [[String: Any]]
    XCTAssertEqual(types.count, 1)

    XCTAssertNil(parsed["queries"])
    XCTAssertNil(parsed["mutations"])

    let userDef = types[0]
    XCTAssertEqual(userDef["name"] as? String, "User")
    XCTAssertEqual(userDef["description"] as? String, "User in the system")
  }

  func testExportTypesMultipleTypes() throws {
    Schema.registerType("User", fields: [
      "id": ["type": "ID", "nullable": false],
    ])
    Schema.registerType("Post", fields: [
      "id": ["type": "ID", "nullable": false],
      "title": ["type": "String", "nullable": false],
    ])

    let json = Schema.exportTypes(pretty: true)
    let data = json.data(using: .utf8)!
    let parsed = try JSONSerialization.jsonObject(with: data) as! [String: Any]

    let types = parsed["types"] as! [[String: Any]]
    XCTAssertEqual(types.count, 2)

    let typeNames = types.compactMap { $0["name"] as? String }
    XCTAssert(typeNames.contains("User"))
    XCTAssert(typeNames.contains("Post"))
  }

  func testExportTypesNoQueries() throws {
    Schema.registerType("User", fields: [
      "id": ["type": "ID", "nullable": false],
    ])

    let json = Schema.exportTypes(pretty: true)
    let data = json.data(using: .utf8)!
    let parsed = try JSONSerialization.jsonObject(with: data) as! [String: Any]

    XCTAssertNotNil(parsed["types"])
    XCTAssertNil(parsed["queries"])
    XCTAssertNil(parsed["mutations"])
  }

  func testExportTypesCompactFormat() throws {
    Schema.registerType("User", fields: [
      "id": ["type": "ID", "nullable": false],
    ])

    let compact = Schema.exportTypes(false)
    let pretty = Schema.exportTypes(true)

    // Both should be valid JSON
    let compactData = compact.data(using: .utf8)!
    let prettyData = pretty.data(using: .utf8)!
    XCTAssertNoThrow(try JSONSerialization.jsonObject(with: compactData))
    XCTAssertNoThrow(try JSONSerialization.jsonObject(with: prettyData))

    // Compact should be smaller
    XCTAssertLessThanOrEqual(compact.count, pretty.count)
  }

  func testExportTypesPrettyFormat() throws {
    Schema.registerType("User", fields: [
      "id": ["type": "ID", "nullable": false],
    ])

    let json = Schema.exportTypes(true)
    XCTAssert(json.contains("\n"))

    let data = json.data(using: .utf8)!
    XCTAssertNoThrow(try JSONSerialization.jsonObject(with: data))
  }

  func testExportTypesFile() throws {
    Schema.registerType("User", fields: [
      "id": ["type": "ID", "nullable": false],
      "name": ["type": "String", "nullable": false],
    ])

    let tmpFile = "/tmp/fraiseql_types_test_swift.json"
    try? FileManager.default.removeItem(atPath: tmpFile)

    Schema.exportTypesFile(tmpFile)

    XCTAssert(FileManager.default.fileExists(atPath: tmpFile))

    let content = try String(contentsOfFile: tmpFile, encoding: .utf8)
    let data = content.data(using: .utf8)!
    let parsed = try JSONSerialization.jsonObject(with: data) as! [String: Any]

    XCTAssertNotNil(parsed["types"])
    let types = parsed["types"] as! [[String: Any]]
    XCTAssertEqual(types.count, 1)

    try? FileManager.default.removeItem(atPath: tmpFile)
  }

  func testExportTypesEmpty() throws {
    let json = Schema.exportTypes(true)
    let data = json.data(using: .utf8)!
    let parsed = try JSONSerialization.jsonObject(with: data) as! [String: Any]

    XCTAssertNotNil(parsed["types"])
    let types = parsed["types"] as! [[String: Any]]
    XCTAssertEqual(types.count, 0)
  }
}
