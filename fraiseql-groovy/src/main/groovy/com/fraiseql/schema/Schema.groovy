package com.fraiseql.schema

import groovy.json.JsonBuilder
import groovy.json.JsonSlurper
import java.nio.file.Files
import java.nio.file.Paths

class TypeInfo {
  String name
  Map<String, Map<String, Object>> fields
  String description

  TypeInfo(String name, Map<String, Map<String, Object>> fields, String description = null) {
    this.name = name
    this.fields = fields
    this.description = description
  }
}

class SchemaRegistry {
  private static SchemaRegistry instance
  private Map<String, TypeInfo> types = [:]

  private SchemaRegistry() {}

  static SchemaRegistry getInstance() {
    if (!instance) {
      instance = new SchemaRegistry()
    }
    return instance
  }

  void register(String name, TypeInfo info) {
    types[name] = info
  }

  List<String> getTypeNames() {
    return types.keySet().toList()
  }

  TypeInfo getType(String name) {
    return types[name]
  }

  void clear() {
    types.clear()
  }
}

class Schema {
  private static SchemaRegistry registry = SchemaRegistry.getInstance()

  static void registerType(String name, Map<String, Map<String, Object>> fields, String description = null) {
    registry.register(name, new TypeInfo(name, fields, description))
  }

  static String exportTypes(boolean pretty = true) {
    List<String> typeNames = registry.getTypeNames()
    List<Map<String, Object>> types = []

    typeNames.each { typeName ->
      TypeInfo typeInfo = registry.getType(typeName)
      if (typeInfo) {
        List<Map<String, Object>> fieldsArray = []
        typeInfo.fields.each { fieldName, fieldConfig ->
          fieldsArray << [
            name: fieldName,
            type: fieldConfig.type ?: 'String',
            nullable: fieldConfig.nullable ?: false
          ]
        }

        Map<String, Object> typeObj = [
          name: typeName,
          fields: fieldsArray
        ]

        if (typeInfo.description) {
          typeObj.description = typeInfo.description
        }

        types << typeObj
      }
    }

    Map<String, Object> schema = [types: types]

    if (pretty) {
      return JsonBuilder.builder(schema).prettyPrint()
    } else {
      return new JsonBuilder(schema).toString()
    }
  }

  static void exportTypesFile(String outputPath) {
    try {
      String json = exportTypes(true)
      Path path = Paths.get(outputPath)

      Files.createDirectories(path.parent)
      Files.writeString(path, json)

      int typesCount = registry.getTypeNames().size()
      println("✅ Types exported to ${outputPath}")
      println("   Types: ${typesCount}")
      println()
      println("🎯 Next steps:")
      println("   1. fraiseql compile fraiseql.toml --types ${outputPath}")
      println("   2. This merges types with TOML configuration")
      println("   3. Result: schema.compiled.json with types + all config")
    } catch (Exception e) {
      throw new RuntimeException("Failed to write types file: ${outputPath}")
    }
  }

  static void reset() {
    registry.clear()
  }

  static List<String> getTypeNames() {
    return registry.getTypeNames()
  }
}
