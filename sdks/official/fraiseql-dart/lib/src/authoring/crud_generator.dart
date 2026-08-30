/// Generates CRUD queries and mutations for FraiseQL types.
///
/// When a type has `crud: true`, this generator produces standard read, create,
/// update, and delete operations following FraiseQL conventions:
///
/// - Read: query `{snake}` (get by PK) + query `{snakes}` (list with auto_params)
/// - Create: mutation `create_{snake}` taking `input: Create{Type}Input!`
/// - Update: mutation `update_{snake}` taking `input: Update{Type}Input!`
/// - Delete: mutation `delete_{snake}` with PK only
///
/// The two input objects are the shape six of the nine generating SDKs emit and the one
/// `docs/architecture/mutation-response.md` documents. This generator emitted flat
/// arguments and no input types, so the same declaration produced a different GraphQL API
/// in Dart than in Python (#1246) — though nothing could observe that, because until
/// #1241 nothing called this class at all.
class CrudGenerator {
  /// Convert a PascalCase name to snake_case.
  static String pascalToSnake(String name) {
    return name
        .replaceAllMapped(
          RegExp(r'(?<!^)([A-Z])'),
          (m) => '_${m[1]}',
        )
        .toLowerCase();
  }

  /// Convert a snake_case name to camelCase.
  ///
  /// Idempotent: already-camelCase strings are returned unchanged.
  static String snakeToCamel(String name) {
    return name.replaceAllMapped(
      RegExp(r'_([a-z])'),
      (m) => m[1]!.toUpperCase(),
    );
  }

  /// Apply basic English pluralization rules to a snake_case name.
  ///
  /// Rules (ordered):
  /// 1. Already ends in 's' (but not 'ss') -> no change
  /// 2. Ends in 'ss', 'sh', 'ch', 'x', 'z' -> append 'es'
  /// 3. Ends in consonant + 'y' -> replace 'y' with 'ies'
  /// 4. Default -> append 's'
  static String pluralize(String name) {
    if (name.endsWith('s') && !name.endsWith('ss')) return name;
    for (final suffix in ['ss', 'sh', 'ch', 'x', 'z']) {
      if (name.endsWith(suffix)) return '${name}es';
    }
    if (name.length >= 2 &&
        name.endsWith('y') &&
        !'aeiou'.contains(name[name.length - 2])) {
      return '${name.substring(0, name.length - 1)}ies';
    }
    return '${name}s';
  }

  /// Generate CRUD operations for a type.
  ///
  /// Returns a map with `'queries'`, `'mutations'` and `'input_types'` keys, each
  /// containing a list of definition maps suitable for schema JSON serialization. The
  /// input types carry `is_input` — this SDK's route to an input object, matching how
  /// `FraiseQLSchema.type(isInput: true)` declares one by hand.
  ///
  /// [typeName] is the PascalCase GraphQL type name.
  /// [fields] is a list of field maps with `name`, `type`, `nullable` and optionally
  /// `computed` keys. A computed field is server-assigned — a slug, a view aggregation —
  /// so a client cannot supply one and it is omitted from both input objects.
  /// [sqlSource] overrides the default view name (`v_{snake}`).
  /// [cascade] when true, generated mutations include `cascade: true`.
  /// [operations] limits generation to a subset of `read`, `create`, `update`, `delete`;
  /// null means all four, matching `crud: true`.
  static Map<String, List<Map<String, dynamic>>> generate({
    required String typeName,
    required List<Map<String, dynamic>> fields,
    String? sqlSource,
    bool cascade = false,
    List<String>? operations,
  }) {
    if (fields.isEmpty) {
      throw ArgumentError(
        'Type "$typeName" has no fields; cannot generate CRUD operations.',
      );
    }

    const known = {'read', 'create', 'update', 'delete'};
    final ops = operations == null ? known : operations.toSet();
    final unknown = ops.difference(known);
    if (unknown.isNotEmpty) {
      throw ArgumentError(
        'Type "$typeName" declares unknown CRUD operation(s) ${unknown.toList()..sort()}; '
        'expected any of read, create, update, delete.',
      );
    }

    final snake = pascalToSnake(typeName);
    final view = sqlSource ?? 'v_$snake';
    final pkField = fields.first;

    final queries = <Map<String, dynamic>>[];
    final mutations = <Map<String, dynamic>>[];
    final inputTypes = <Map<String, dynamic>>[];

    // Get by ID
    if (ops.contains('read')) {
      queries.add({
        'name': snakeToCamel(snake),
        'return_type': typeName,
        'returns_list': false,
        'nullable': true,
        'arguments': [
          {
            'name': snakeToCamel(pkField['name'] as String),
            'type': pkField['type'],
            'nullable': false,
          },
        ],
        'description': 'Get $typeName by ID.',
        'sql_source': view,
      });

      // List
      queries.add({
        'name': snakeToCamel(pluralize(snake)),
        'return_type': typeName,
        'returns_list': true,
        'nullable': false,
        'arguments': <Map<String, dynamic>>[],
        'description': 'List $typeName records.',
        'sql_source': view,
        'auto_params': {
          'where': true,
          'order_by': true,
          'limit': true,
          'offset': true,
        },
      });
    }

    if (ops.contains('create')) {
      // Create — every non-computed field, in an input object.
      final createInputName = 'Create${typeName}Input';
      inputTypes.add({
        'name': createInputName,
        'is_input': true,
        'description': 'Input for creating a new $typeName.',
        'fields': fields
            .where((f) => f['computed'] != true)
            .map((f) => {
                  'name': snakeToCamel(f['name'] as String),
                  'type': f['type'],
                  'nullable': f['nullable'] ?? false,
                })
            .toList(),
      });
      final createMutation = <String, dynamic>{
        'name': snakeToCamel('create_$snake'),
        'return_type': typeName,
        'returns_list': false,
        'nullable': false,
        'arguments': [
          {'name': 'input', 'type': createInputName, 'nullable': false},
        ],
        'description': 'Create a new $typeName.',
        'sql_source': 'fn_create_$snake',
        'operation': 'INSERT',
      };
      if (cascade) createMutation['cascade'] = true;
      mutations.add(createMutation);
    }

    if (ops.contains('update')) {
      // Update — PK required, every other non-computed field optional, in an input object.
      final updateInputName = 'Update${typeName}Input';
      inputTypes.add({
        'name': updateInputName,
        'is_input': true,
        'description': 'Input for updating an existing $typeName.',
        'fields': <Map<String, dynamic>>[
          {
            'name': snakeToCamel(pkField['name'] as String),
            'type': pkField['type'],
            'nullable': false,
          },
          ...fields.skip(1).where((f) => f['computed'] != true).map((f) => {
                'name': snakeToCamel(f['name'] as String),
                'type': f['type'],
                'nullable': true,
              }),
        ],
      });
      final updateMutation = <String, dynamic>{
        'name': snakeToCamel('update_$snake'),
        'return_type': typeName,
        'returns_list': false,
        'nullable': true,
        'arguments': [
          {'name': 'input', 'type': updateInputName, 'nullable': false},
        ],
        'description': 'Update an existing $typeName.',
        'sql_source': 'fn_update_$snake',
        'operation': 'UPDATE',
      };
      if (cascade) updateMutation['cascade'] = true;
      mutations.add(updateMutation);
    }

    if (ops.contains('delete')) {
      // Delete
      final deleteMutation = <String, dynamic>{
        'name': snakeToCamel('delete_$snake'),
        'return_type': typeName,
        'returns_list': false,
        'nullable': false,
        'arguments': [
          {
            'name': snakeToCamel(pkField['name'] as String),
            'type': pkField['type'],
            'nullable': false,
          },
        ],
        'description': 'Delete a $typeName.',
        'sql_source': 'fn_delete_$snake',
        'operation': 'DELETE',
      };
      if (cascade) deleteMutation['cascade'] = true;
      mutations.add(deleteMutation);
    }

    return {
      'queries': queries,
      'mutations': mutations,
      'input_types': inputTypes,
    };
  }
}
