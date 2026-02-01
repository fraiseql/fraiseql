import Foundation

typealias FieldConfig = [String: Any]
typealias TypeFields = [String: FieldConfig]

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
        fieldsArray.append([
          "name": fieldName,
          "type": fieldType,
          "nullable": nullable
        ])
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
}

extension URL {
  func parent() -> URL {
    return deletingLastPathComponent()
  }
}
