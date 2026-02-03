import XCTest
@testable import FraiseQL

final class Phase18Cycle19ScopeExtractionTests: XCTestCase {

  override func setUp() {
    super.setUp()
    Schema.reset()
  }

  // MARK: - Field Creation Tests (3 tests)

  func testFieldCreationWithAllProperties() throws {
    let fields: TypeFields = [
      "email": [
        "type": "String",
        "nullable": false,
        "description": "User email address",
        "requiresScope": "read:user.email"
      ]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    XCTAssertNotNil(typeInfo)

    let emailField = typeInfo!.fields["email"]
    XCTAssertNotNil(emailField)
    XCTAssertEqual(emailField?["type"] as? String, "String")
    XCTAssertEqual(emailField?["nullable"] as? Bool, false)
    XCTAssertEqual(emailField?["description"] as? String, "User email address")
    XCTAssertEqual(emailField?["requiresScope"] as? String, "read:user.email")
  }

  func testFieldCreationWithMinimalProperties() throws {
    let fields: TypeFields = [
      "id": ["type": "Int"]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    XCTAssertNotNil(typeInfo)

    let idField = typeInfo!.fields["id"]
    XCTAssertEqual(idField?["type"] as? String, "Int")
    XCTAssertNil(idField?["requiresScope"])
    XCTAssertNil(idField?["requiresScopes"])
  }

  func testFieldWithMetadataPreservation() throws {
    let fields: TypeFields = [
      "password": [
        "type": "String",
        "nullable": false,
        "description": "Hashed password",
        "requiresScope": "admin:user.*"
      ]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    let passwordField = typeInfo!.fields["password"]
    XCTAssertEqual(passwordField?["type"] as? String, "String")
    XCTAssertEqual(passwordField?["requiresScope"] as? String, "admin:user.*")
    XCTAssertEqual(passwordField?["description"] as? String, "Hashed password")
  }

  // MARK: - Single Scope Requirement Tests (3 tests)

  func testFieldWithSingleScopeFormat() throws {
    let fields: TypeFields = [
      "email": [
        "type": "String",
        "requiresScope": "read:user.email"
      ]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    let emailField = typeInfo!.fields["email"]
    XCTAssertEqual(emailField?["requiresScope"] as? String, "read:user.email")
    XCTAssertNil(emailField?["requiresScopes"])
  }

  func testFieldWithWildcardResourceScope() throws {
    let fields: TypeFields = [
      "profile": [
        "type": "Object",
        "requiresScope": "read:User.*"
      ]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    let profileField = typeInfo!.fields["profile"]
    XCTAssertEqual(profileField?["requiresScope"] as? String, "read:User.*")
  }

  func testFieldWithGlobalWildcardScope() throws {
    let fields: TypeFields = [
      "secret": [
        "type": "String",
        "requiresScope": "admin:*"
      ]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    let secretField = typeInfo!.fields["secret"]
    XCTAssertEqual(secretField?["requiresScope"] as? String, "admin:*")
  }

  // MARK: - Multiple Scopes Array Tests (3 tests)

  func testFieldWithMultipleScopesArray() throws {
    let fields: TypeFields = [
      "email": [
        "type": "String",
        "requiresScopes": ["read:user.email", "write:user.email"]
      ]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    let emailField = typeInfo!.fields["email"]
    let scopes = emailField?["requiresScopes"] as? [String]
    XCTAssertNotNil(scopes)
    XCTAssertEqual(scopes?.count, 2)
    XCTAssertTrue(scopes!.contains("read:user.email"))
    XCTAssertTrue(scopes!.contains("write:user.email"))
  }

  func testFieldWithSingleElementScopesArray() throws {
    let fields: TypeFields = [
      "profile": [
        "type": "Object",
        "requiresScopes": ["read:user.profile"]
      ]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    let profileField = typeInfo!.fields["profile"]
    let scopes = profileField?["requiresScopes"] as? [String]
    XCTAssertNotNil(scopes)
    XCTAssertEqual(scopes?.count, 1)
    XCTAssertEqual(scopes?[0], "read:user.profile")
  }

  func testFieldWithComplexScopesArray() throws {
    let fields: TypeFields = [
      "data": [
        "type": "String",
        "requiresScopes": ["read:user.email", "write:user.*", "admin:*"]
      ]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    let dataField = typeInfo!.fields["data"]
    let scopes = dataField?["requiresScopes"] as? [String]
    XCTAssertNotNil(scopes)
    XCTAssertEqual(scopes?.count, 3)
  }

  // MARK: - Scope Pattern Validation Tests (6 tests)

  func testScopeValidatorValidatesSpecificFieldScope() throws {
    XCTAssertTrue(ScopeValidator.validate("read:user.email"))
  }

  func testScopeValidatorValidatesResourceWildcardScope() throws {
    XCTAssertTrue(ScopeValidator.validate("read:User.*"))
  }

  func testScopeValidatorValidatesGlobalAdminWildcard() throws {
    XCTAssertTrue(ScopeValidator.validate("admin:*"))
  }

  func testScopeValidatorRejectsScopeMissingColon() throws {
    XCTAssertFalse(ScopeValidator.validate("readuser"))
  }

  func testScopeValidatorRejectsActionWithHyphen() throws {
    XCTAssertFalse(ScopeValidator.validate("read-all:user"))
  }

  func testScopeValidatorRejectsResourceWithHyphen() throws {
    XCTAssertFalse(ScopeValidator.validate("read:user-data"))
  }

  // MARK: - Schema Registry Tests (3 tests)

  func testSchemaRegistersTypeWithFieldsAndScopes() throws {
    let fields: TypeFields = [
      "id": ["type": "Int", "nullable": false],
      "email": ["type": "String", "nullable": false, "requiresScope": "read:user.email"]
    ]

    Schema.registerType("User", fields: fields)

    let typeNames = Schema.getTypeNames()
    XCTAssertTrue(typeNames.contains("User"))
  }

  func testSchemaExtractsScopedFieldsFromRegistry() throws {
    let fields: TypeFields = [
      "id": ["type": "Int", "nullable": false],
      "email": ["type": "String", "nullable": false, "requiresScope": "read:user.email"],
      "password": ["type": "String", "nullable": false, "requiresScope": "admin:user.password"]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    XCTAssertNotNil(typeInfo)
    XCTAssertEqual(typeInfo!.fields["email"]?["requiresScope"] as? String, "read:user.email")
    XCTAssertEqual(typeInfo!.fields["password"]?["requiresScope"] as? String, "admin:user.password")
  }

  func testSchemaHandlesMultipleTypesWithDifferentScopes() throws {
    Schema.registerType("User", fields: [
      "id": ["type": "Int"],
      "email": ["type": "String", "requiresScope": "read:user.email"]
    ])

    Schema.registerType("Post", fields: [
      "id": ["type": "Int"],
      "content": ["type": "String", "requiresScope": "read:post.content"]
    ])

    let typeNames = Schema.getTypeNames()
    XCTAssertEqual(typeNames.count, 2)
    XCTAssertTrue(typeNames.contains("User"))
    XCTAssertTrue(typeNames.contains("Post"))
  }

  // MARK: - JSON Export Tests (2 tests)

  func testSchemaExportIncludesScopeInFieldJSON() throws {
    let fields: TypeFields = [
      "email": ["type": "String", "nullable": false, "requiresScope": "read:user.email"]
    ]

    Schema.registerType("User", fields: fields)
    let json = Schema.exportTypes(pretty: false)

    XCTAssertTrue(json.contains("User"))
    XCTAssertTrue(json.contains("email"))
    XCTAssertTrue(json.contains("read:user.email"))
    XCTAssertTrue(json.contains("requiresScope"))
  }

  func testSchemaExportMultipleTypesWithScopes() throws {
    Schema.registerType("User", fields: [
      "id": ["type": "Int"],
      "email": ["type": "String", "requiresScope": "read:user.email"]
    ])

    Schema.registerType("Post", fields: [
      "id": ["type": "Int"],
      "content": ["type": "String", "requiresScope": "read:post.content"]
    ])

    let json = Schema.exportTypes(pretty: false)

    XCTAssertTrue(json.contains("User"))
    XCTAssertTrue(json.contains("Post"))
    XCTAssertTrue(json.contains("read:user.email"))
    XCTAssertTrue(json.contains("read:post.content"))
  }

  // MARK: - Conflicting Scope/Scopes Tests (2 tests)

  func testFieldWithBothScopeAndScopes() throws {
    let fields: TypeFields = [
      "email": [
        "type": "String",
        "requiresScope": "read:user.email",
        "requiresScopes": ["write:user.email"]
      ]
    ]

    Schema.registerType("User", fields: fields)

    let typeInfo = Schema.getType("User")
    let emailField = typeInfo!.fields["email"]
    XCTAssertEqual(emailField?["requiresScope"] as? String, "read:user.email")
    let scopes = emailField?["requiresScopes"] as? [String]
    XCTAssertNotNil(scopes)
    XCTAssertEqual(scopes?.count, 1)
  }

  func testScopeValidatorRejectsEmptyScopeString() throws {
    XCTAssertFalse(ScopeValidator.validate(""))
  }

}
