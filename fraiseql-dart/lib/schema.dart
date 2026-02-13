import 'dart:convert';
import 'dart:io';

typedef FieldConfig = Map<String, dynamic>;
typedef TypeFields = Map<String, FieldConfig>;

class TypeInfo {
  final String name;
  final TypeFields fields;
  final String? description;

  TypeInfo({
    required this.name,
    required this.fields,
    this.description,
  });
}

class SchemaRegistry {
  static final SchemaRegistry _instance = SchemaRegistry._internal();
  final Map<String, TypeInfo> _types = {};

  factory SchemaRegistry() {
    return _instance;
  }

  SchemaRegistry._internal();

  void register(String name, TypeInfo info) {
    _types[name] = info;
  }

  List<String> getTypeNames() => _types.keys.toList();

  TypeInfo? getType(String name) => _types[name];

  void clear() => _types.clear();
}

/// Validates scope format: action:resource
/// Examples: read:user.email, admin:*, write:Post.*
bool _validateScope(String scope) {
  if (scope.isEmpty) return false;
  if (scope == '*') return true;

  final parts = scope.split(':');
  if (parts.length != 2) return false;

  final action = parts[0];
  final resource = parts[1];

  if (action.isEmpty || resource.isEmpty) return false;

  return _isValidAction(action) && _isValidResource(resource);
}

/// Validates action matches pattern: [a-zA-Z_][a-zA-Z0-9_]*
bool _isValidAction(String action) {
  if (action.isEmpty) return false;
  final first = action[0];
  if (!RegExp(r'[a-zA-Z_]').hasMatch(first)) return false;

  for (int i = 1; i < action.length; i++) {
    if (!RegExp(r'[a-zA-Z0-9_]').hasMatch(action[i])) return false;
  }
  return true;
}

/// Validates resource matches pattern: [a-zA-Z_][a-zA-Z0-9_.]*|*
bool _isValidResource(String resource) {
  if (resource == '*') return true;
  if (resource.isEmpty) return false;

  final first = resource[0];
  if (!RegExp(r'[a-zA-Z_]').hasMatch(first)) return false;

  for (int i = 1; i < resource.length; i++) {
    if (!RegExp(r'[a-zA-Z0-9_.]').hasMatch(resource[i])) return false;
  }
  return true;
}

class Schema {
  static final _registry = SchemaRegistry();

  static void registerType(
    String name,
    TypeFields fields, {
    String? description,
  }) {
    // Validate scope fields in all fields
    for (final fieldEntry in fields.entries) {
      final fieldConfig = fieldEntry.value;
      final hasScope = fieldConfig.containsKey('requires_scope');
      final hasScopes = fieldConfig.containsKey('requires_scopes');

      // Check for conflicting scope and scopes
      if (hasScope && hasScopes) {
        throw FormatException(
          'Field "${fieldEntry.key}" cannot have both requires_scope and requires_scopes',
        );
      }

      // Validate requires_scope if present
      if (hasScope) {
        final scope = fieldConfig['requires_scope'];
        if (scope is! String) {
          throw FormatException(
            'Field "${fieldEntry.key}" requires_scope must be a string',
          );
        }
        if (!_validateScope(scope)) {
          throw FormatException(
            'Field "${fieldEntry.key}" has invalid scope format: "$scope"',
          );
        }
      }

      // Validate requires_scopes if present
      if (hasScopes) {
        final scopes = fieldConfig['requires_scopes'];
        if (scopes is! List) {
          throw FormatException(
            'Field "${fieldEntry.key}" requires_scopes must be a list',
          );
        }
        if (scopes.isEmpty) {
          throw FormatException(
            'Field "${fieldEntry.key}" requires_scopes cannot be empty',
          );
        }
        for (final scope in scopes) {
          if (scope is! String) {
            throw FormatException(
              'Field "${fieldEntry.key}" requires_scopes contains non-string value',
            );
          }
          if (!_validateScope(scope)) {
            throw FormatException(
              'Field "${fieldEntry.key}" has invalid scope in requires_scopes: "$scope"',
            );
          }
        }
      }
    }

    _registry.register(
      name,
      TypeInfo(name: name, fields: fields, description: description),
    );
  }

  static String exportTypes({bool pretty = true}) {
    final typeNames = _registry.getTypeNames();
    final types = <Map<String, dynamic>>[];

    for (final typeName in typeNames) {
      final typeInfo = _registry.getType(typeName);
      if (typeInfo == null) continue;

      final fieldsArray = <Map<String, dynamic>>[];
      typeInfo.fields.forEach((fieldName, fieldConfig) {
        final field = <String, dynamic>{
          'name': fieldName,
          'type': fieldConfig['type'] ?? 'String',
          'nullable': fieldConfig['nullable'] ?? false,
        };

        // Add requires_scope if present
        if (fieldConfig.containsKey('requires_scope')) {
          field['requires_scope'] = fieldConfig['requires_scope'];
        }

        // Add requires_scopes if present
        if (fieldConfig.containsKey('requires_scopes')) {
          field['requires_scopes'] = fieldConfig['requires_scopes'];
        }

        fieldsArray.add(field);
      });

      final typeObj = {
        'name': typeName,
        'fields': fieldsArray,
      };
      if (typeInfo.description != null) {
        typeObj['description'] = typeInfo.description;
      }
      types.add(typeObj);
    }

    final schema = {'types': types};
    final json = jsonEncode(schema);

    if (pretty) {
      return JsonEncoder.withIndent('  ').convert(schema);
    }
    return json;
  }

  static void exportTypesFile(String outputPath) {
    try {
      final json = exportTypes(pretty: true);
      final file = File(outputPath);

      file.parent.createSync(recursive: true);
      file.writeAsStringSync(json);

      final typesCount = _registry.getTypeNames().length;
      print('✅ Types exported to $outputPath');
      print('   Types: $typesCount');
      print('');
      print('🎯 Next steps:');
      print('   1. fraiseql compile fraiseql.toml --types $outputPath');
      print('   2. This merges types with TOML configuration');
      print('   3. Result: schema.compiled.json with types + all config');
    } catch (e) {
      throw Exception('Failed to write types file: $outputPath');
    }
  }

  static void reset() => _registry.clear();

  static List<String> getTypeNames() => _registry.getTypeNames();

  static TypeInfo? getType(String name) => _registry.getType(name);
}
