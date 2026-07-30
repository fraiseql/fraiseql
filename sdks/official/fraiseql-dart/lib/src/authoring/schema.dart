import 'dart:convert';
import 'dart:io';

/// Builds the intermediate `schema.json` document consumed by `fraiseql compile`.
///
/// This is the API the README's Quick Start has always documented. It did not exist:
/// `FraiseQLSchema`, `FieldType` and `schema.exportJson` were absent from the package,
/// and `lib/fraiseql.dart` exported neither them nor the CRUD generator, so the
/// documented example failed to compile on its first two lines. Dart shipped annotations
/// that nothing read and no schema export path at all (#853).
///
/// Every key emitted here is the one the compiler reads. The compiler denies unknown
/// fields, so a misspelling is a compile error naming the key rather than a silent drop.
///
/// ```dart
/// final schema = FraiseQLSchema();
///
/// schema.type('User', sqlSource: 'v_user', fields: {
///   'id': FieldType.id(nullable: false),
///   'email': FieldType.string(nullable: false),
/// });
///
/// schema.query('users', returnType: 'User', returnsList: true, sqlSource: 'v_user');
///
/// schema.exportJson('schema.json');
/// ```
class FraiseQLSchema {
  final List<Map<String, Object?>> _types = [];
  final List<Map<String, Object?>> _enums = [];
  final List<Map<String, Object?>> _queries = [];
  final List<Map<String, Object?>> _mutations = [];

  /// Declares a GraphQL object type backed by a SQL view.
  ///
  /// `isInput: true` declares a GraphQL input object instead. An input object has no
  /// backing relation, so `sqlSource` is refused on one — the compiler rejects a type
  /// that declares both.
  Map<String, Object?> type(
    String name, {
    required Map<String, FieldType> fields,
    String? sqlSource,
    String? description,
    bool relay = false,
    bool isError = false,
    bool isInput = false,
  }) {
    if (isInput && sqlSource != null) {
      throw ArgumentError(
        "type '$name': an input type must not declare sqlSource — an input object has "
        'no backing view.',
      );
    }

    final definition = <String, Object?>{
      'name': name,
      'fields': [
        for (final entry in fields.entries) entry.value._toJson(entry.key),
      ],
    };
    if (sqlSource != null) definition['sql_source'] = sqlSource;
    if (description != null) definition['description'] = description;
    if (relay) definition['relay'] = true;
    if (isError) definition['is_error'] = true;
    if (isInput) definition['is_input'] = true;

    _types.add(definition);
    return definition;
  }

  /// Declares a GraphQL enum type.
  Map<String, Object?> enumType(
    String name,
    List<String> values, {
    String? description,
  }) {
    final definition = <String, Object?>{
      'name': name,
      'values': [
        for (final value in values) {'name': value},
      ],
    };
    if (description != null) definition['description'] = description;

    _enums.add(definition);
    return definition;
  }

  /// Declares a GraphQL query.
  ///
  /// `inject` maps a SQL parameter to a `"jwt:<claim>"` source and is emitted under
  /// `inject_params` — the key the compiler reads.
  Map<String, Object?> query(
    String name, {
    required String returnType,
    String? sqlSource,
    bool returnsList = false,
    bool nullable = false,
    Map<String, FieldType> arguments = const {},
    String? description,
    int? cacheTtlSeconds,
    String? requiresRole,
    Map<String, String> inject = const {},
  }) {
    final definition = <String, Object?>{
      'name': name,
      'return_type': returnType,
      'returns_list': returnsList,
      'nullable': nullable,
      'arguments': [
        for (final entry in arguments.entries) entry.value._toJson(entry.key),
      ],
    };
    if (sqlSource != null) definition['sql_source'] = sqlSource;
    if (description != null) definition['description'] = description;
    if (cacheTtlSeconds != null)
      definition['cache_ttl_seconds'] = cacheTtlSeconds;
    if (requiresRole != null) definition['requires_role'] = requiresRole;
    if (inject.isNotEmpty) definition['inject_params'] = _injectParams(inject);

    _queries.add(definition);
    return definition;
  }

  /// Declares a GraphQL mutation.
  ///
  /// `invalidatesViews` and `invalidatesFactTables` are what connect a write to the
  /// cached reads of what it wrote; without them a new row stays invisible for the whole
  /// of a reader's TTL.
  Map<String, Object?> mutation(
    String name, {
    required String returnType,
    String? sqlSource,
    String? operation,
    bool returnsList = false,
    bool nullable = false,
    Map<String, FieldType> arguments = const {},
    String? description,
    String? requiresRole,
    Map<String, String> inject = const {},
    List<String> invalidatesViews = const [],
    List<String> invalidatesFactTables = const [],
  }) {
    final definition = <String, Object?>{
      'name': name,
      'return_type': returnType,
      'returns_list': returnsList,
      'nullable': nullable,
      'arguments': [
        for (final entry in arguments.entries) entry.value._toJson(entry.key),
      ],
    };
    if (sqlSource != null) definition['sql_source'] = sqlSource;
    if (operation != null) definition['operation'] = operation;
    if (description != null) definition['description'] = description;
    if (requiresRole != null) definition['requires_role'] = requiresRole;
    if (inject.isNotEmpty) definition['inject_params'] = _injectParams(inject);
    if (invalidatesViews.isNotEmpty) {
      definition['invalidates_views'] = invalidatesViews;
    }
    if (invalidatesFactTables.isNotEmpty) {
      definition['invalidates_fact_tables'] = invalidatesFactTables;
    }

    _mutations.add(definition);
    return definition;
  }

  /// The schema as a JSON-encodable map, in the intermediate format.
  ///
  /// Empty sections are omitted rather than emitted as `null`: a `null` array is
  /// rejected by the compiler with `invalid type: null, expected a sequence` and no key
  /// name.
  Map<String, Object?> toJson() {
    final document = <String, Object?>{'version': '2.0.0', 'types': _types};
    if (_enums.isNotEmpty) document['enums'] = _enums;
    if (_queries.isNotEmpty) document['queries'] = _queries;
    if (_mutations.isNotEmpty) document['mutations'] = _mutations;
    return document;
  }

  /// Writes the schema to [path], ready for `fraiseql compile`.
  void exportJson(String path) {
    final json = const JsonEncoder.withIndent('  ').convert(toJson());
    File(path).writeAsStringSync('$json\n');
  }

  /// Normalises a `{param: "jwt:claim"}` map into the nested form the compiler reads.
  static Map<String, Object?> _injectParams(Map<String, String> inject) {
    return {
      for (final entry in inject.entries)
        entry.key: () {
          final parts = entry.value.split(':');
          if (parts.length < 2 || parts[0].isEmpty || parts[1].isEmpty) {
            throw ArgumentError(
              "inject_params['${entry.key}'] must be \"<source>:<claim>\", for example "
              '"jwt:tenant_id", got "${entry.value}"',
            );
          }
          return {'source': parts[0], 'claim': parts.sublist(1).join(':')};
        }(),
    };
  }
}

/// A field or argument type, with its nullability and access metadata.
///
/// Named constructors mirror the GraphQL scalar names; [FieldType.named] references a
/// type declared elsewhere in the schema.
class FieldType {
  /// The GraphQL type name, e.g. `String`, `ID`, `Order`.
  final String type;

  /// Whether the field or argument accepts null. Defaults to GraphQL's own default for
  /// an unadorned type.
  final bool nullable;

  /// Optional description, carried into introspection.
  final String? description;

  /// JWT scope required to read this field.
  final String? requiresScope;

  /// Policy when the caller lacks [requiresScope]: `reject` (default) or `mask`.
  final String? onDeny;

  const FieldType.named(
    this.type, {
    this.nullable = true,
    this.description,
    this.requiresScope,
    this.onDeny,
  });

  /// GraphQL `ID`.
  const FieldType.id({
    bool nullable = true,
    String? description,
    String? requiresScope,
    String? onDeny,
  }) : this.named(
          'ID',
          nullable: nullable,
          description: description,
          requiresScope: requiresScope,
          onDeny: onDeny,
        );

  /// GraphQL `String`.
  const FieldType.string({
    bool nullable = true,
    String? description,
    String? requiresScope,
    String? onDeny,
  }) : this.named(
          'String',
          nullable: nullable,
          description: description,
          requiresScope: requiresScope,
          onDeny: onDeny,
        );

  /// GraphQL `Int`. Named `int_` because `int` is a Dart keyword-adjacent type name.
  const FieldType.int_({
    bool nullable = true,
    String? description,
    String? requiresScope,
    String? onDeny,
  }) : this.named(
          'Int',
          nullable: nullable,
          description: description,
          requiresScope: requiresScope,
          onDeny: onDeny,
        );

  /// GraphQL `Float`.
  const FieldType.float({
    bool nullable = true,
    String? description,
    String? requiresScope,
    String? onDeny,
  }) : this.named(
          'Float',
          nullable: nullable,
          description: description,
          requiresScope: requiresScope,
          onDeny: onDeny,
        );

  /// GraphQL `Boolean`.
  const FieldType.boolean({
    bool nullable = true,
    String? description,
    String? requiresScope,
    String? onDeny,
  }) : this.named(
          'Boolean',
          nullable: nullable,
          description: description,
          requiresScope: requiresScope,
          onDeny: onDeny,
        );

  Map<String, Object?> _toJson(String name) {
    final json = <String, Object?>{
      'name': name,
      'type': type,
      'nullable': nullable,
    };
    if (description != null) json['description'] = description;
    if (requiresScope != null) json['requires_scope'] = requiresScope;
    if (onDeny != null) json['on_deny'] = onDeny;
    return json;
  }
}
