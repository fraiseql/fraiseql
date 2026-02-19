import Foundation

typealias FieldConfig = [String: Any]
typealias TypeFields = [String: FieldConfig]

/// Validator for field-level scope format and patterns
///
/// Scope format: action:resource
/// Examples: read:user.email, admin:*, write:Post.*
///
/// Rules:
/// - Action: [a-zA-Z_][a-zA-Z0-9_]*
/// - Resource: [a-zA-Z_][a-zA-Z0-9_.]*|*
enum ScopeValidator {
  /// Validates scope format: action:resource
  ///
  /// - Parameter scope: The scope string to validate
  /// - Returns: true if valid, false otherwise
  static func validate(_ scope: String) -> Bool {
    if scope.isEmpty {
      return false
    }

    if scope == "*" {
      return true
    }

    let parts = scope.split(separator: ":", maxSplits: 1)
    guard parts.count == 2 else {
      return false
    }

    let action = String(parts[0])
    let resource = String(parts[1])

    guard !action.isEmpty, !resource.isEmpty else {
      return false
    }

    return isValidAction(action) && isValidResource(resource)
  }

  /// Validates a list of scopes
  ///
  /// - Parameter scopes: The list of scopes to validate
  /// - Returns: true if all are valid, false otherwise
  static func validateAll(_ scopes: [String]) -> Bool {
    guard !scopes.isEmpty else { return false }
    return scopes.allSatisfy(validate)
  }

  /// Checks if action matches pattern: [a-zA-Z_][a-zA-Z0-9_]*
  private static func isValidAction(_ action: String) -> Bool {
    guard !action.isEmpty else { return false }
    let first = action.first!
    guard first.isLetter || first == "_" else { return false }
    return action.dropFirst().allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" }
  }

  /// Checks if resource matches pattern: [a-zA-Z_][a-zA-Z0-9_.]*|*
  private static func isValidResource(_ resource: String) -> Bool {
    if resource == "*" { return true }
    guard !resource.isEmpty else { return false }
    let first = resource.first!
    guard first.isLetter || first == "_" else { return false }
    return resource.dropFirst().allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" || $0 == "." }
  }
}

struct FieldDefinition: Codable {
  let name: String
  let type: String
  let nullable: Bool
}

struct TypeInfo {
  let name: String
  let fields: TypeFields
  let description: String?
}

final class SchemaRegistry {
  static let shared = SchemaRegistry()
  private var types: [String: TypeInfo] = [:]
  private let lock = NSLock()

  private init() {}

  func register(_ name: String, _ info: TypeInfo) {
    lock.lock()
    defer { lock.unlock() }
    types[name] = info
  }

  func getTypeNames() -> [String] {
    lock.lock()
    defer { lock.unlock() }
    return Array(types.keys)
  }

  func getType(_ name: String) -> TypeInfo? {
    lock.lock()
    defer { lock.unlock() }
    return types[name]
  }

  func clear() {
    lock.lock()
    defer { lock.unlock() }
    types.removeAll()
  }
}

public enum Schema {
  public static func registerType(_ name: String, fields: TypeFields, description: String? = nil) {
    let info = TypeInfo(name: name, fields: fields, description: description)
    SchemaRegistry.shared.register(name, info)
  }

  public static func exportTypes(pretty: Bool = true) -> String {
    let registry = SchemaRegistry.shared
    let typeNames = registry.getTypeNames()

    var types: [[String: Any]] = []
    for typeName in typeNames {
      guard let typeInfo = registry.getType(typeName) else { continue }

      var fieldsArray: [[String: Any]] = []
      for (fieldName, fieldConfig) in typeInfo.fields {
        let fieldType = fieldConfig["type"] as? String ?? "String"
        let nullable = fieldConfig["nullable"] as? Bool ?? false

        var field: [String: Any] = [
          "name": fieldName,
          "type": fieldType,
          "nullable": nullable
        ]

        // Add scope if present
        if let scope = fieldConfig["requiresScope"] as? String {
          field["requiresScope"] = scope
        }

        // Add scopes if present
        if let scopes = fieldConfig["requiresScopes"] as? [String] {
          field["requiresScopes"] = scopes
        }

        fieldsArray.append(field)
      }

      var typeObj: [String: Any] = [
        "name": typeName,
        "fields": fieldsArray
      ]
      if let desc = typeInfo.description {
        typeObj["description"] = desc
      }
      types.append(typeObj)
    }

    let schema: [String: Any] = ["types": types]
    let options: JSONSerialization.WritingOptions = pretty ? [.prettyPrinted, .sortedKeys] : []

    if let jsonData = try? JSONSerialization.data(withJSONObject: schema, options: options),
       let jsonString = String(data: jsonData, encoding: .utf8) {
      return jsonString
    }
    return "{\"types\":[]}"
  }

  public static func exportTypesFile(_ path: String) {
    let json = exportTypes(pretty: true)
    do {
      let fileURL = URL(fileURLWithPath: path)
      try fileURL.parent().createDirectory(withIntermediateDirectories: true, attributes: nil)
      try json.write(toFile: path, atomically: true, encoding: .utf8)

      let count = SchemaRegistry.shared.getTypeNames().count
      print("✅ Types exported to \(path)")
      print("   Types: \(count)")
      print()
      print("🎯 Next steps:")
      print("   1. fraiseql compile fraiseql.toml --types \(path)")
      print("   2. This merges types with TOML configuration")
      print("   3. Result: schema.compiled.json with types + all config")
    } catch {
      fatalError("Failed to write types file: \(path)")
    }
  }

  public static func reset() {
    SchemaRegistry.shared.clear()
  }

  public static func getTypeNames() -> [String] {
    return SchemaRegistry.shared.getTypeNames()
  }

  public static func getType(_ name: String) -> TypeInfo? {
    return SchemaRegistry.shared.getType(name)
  }
}

extension URL {
  func parent() -> URL {
    return deletingLastPathComponent()
  }
}
