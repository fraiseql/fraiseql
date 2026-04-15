/// Generates CRUD queries and mutations for FraiseQL types.
///
/// When a type has `crud: true`, this generator produces standard read, create,
/// update, and delete operations following FraiseQL conventions:
///
/// - Read: query `{snake}` (get by PK) + query `{snakes}` (list with auto_params)
/// - Create: mutation `create_{snake}` with all fields
/// - Update: mutation `update_{snake}` with PK required, other fields nullable
/// - Delete: mutation `delete_{snake}` with PK only
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
  /// Returns a map with `'queries'` and `'mutations'` keys, each containing
  /// a list of operation definition maps suitable for schema JSON serialization.
  ///
  /// [typeName] is the PascalCase GraphQL type name.
  /// [fields] is a list of field maps with `name`, `type`, and `nullable` keys.
  /// [sqlSource] overrides the default view name (`v_{snake}`).
  /// [cascade] when true, generated mutations include `cascade: true`.
  static Map<String, List<Map<String, dynamic>>> generate({
    required String typeName,
    required List<Map<String, dynamic>> fields,
    String? sqlSource,
    bool cascade = false,
  }) {
    if (fields.isEmpty) {
      throw ArgumentError(
        'Type "$typeName" has no fields; cannot generate CRUD operations.',
      );
    }

    final snake = pascalToSnake(typeName);
    final view = sqlSource ?? 'v_$snake';
    final pkField = fields.first;

    final queries = <Map<String, dynamic>>[];
    final mutations = <Map<String, dynamic>>[];

    // Get by ID
    queries.add({
      'name': snakeToCamel(snake),
      'return_type': typeName,
      'returns_list': false,
      'nullable': true,
      'arguments': [
        {'name': snakeToCamel(pkField['name'] as String), 'type': pkField['type'], 'nullable': false},
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

    // Create — exclude computed fields
    final createMutation = <String, dynamic>{
      'name': snakeToCamel('create_$snake'),
      'return_type': typeName,
      'returns_list': false,
      'nullable': false,
      'arguments': fields
          .where((f) => f['computed'] != true)
          .map((f) => {
                'name': snakeToCamel(f['name'] as String),
                'type': f['type'],
                'nullable': f['nullable'] ?? false,
              })
          .toList(),
      'description': 'Create a new $typeName.',
      'sql_source': 'fn_create_$snake',
      'operation': 'INSERT',
    };
    if (cascade) createMutation['cascade'] = true;
    mutations.add(createMutation);

    // Update — PK required, exclude computed non-PK fields
    final updateArgs = <Map<String, dynamic>>[
      {'name': snakeToCamel(pkField['name'] as String), 'type': pkField['type'], 'nullable': false},
      ...fields.skip(1).where((f) => f['computed'] != true).map((f) => {
            'name': snakeToCamel(f['name'] as String),
            'type': f['type'],
            'nullable': true,
          }),
    ];
    final updateMutation = <String, dynamic>{
      'name': snakeToCamel('update_$snake'),
      'return_type': typeName,
      'returns_list': false,
      'nullable': true,
      'arguments': updateArgs,
      'description': 'Update an existing $typeName.',
      'sql_source': 'fn_update_$snake',
      'operation': 'UPDATE',
    };
    if (cascade) updateMutation['cascade'] = true;
    mutations.add(updateMutation);

    // Delete
    final deleteMutation = <String, dynamic>{
      'name': snakeToCamel('delete_$snake'),
      'return_type': typeName,
      'returns_list': false,
      'nullable': false,
      'arguments': [
        {'name': snakeToCamel(pkField['name'] as String), 'type': pkField['type'], 'nullable': false},
      ],
      'description': 'Delete a $typeName.',
      'sql_source': 'fn_delete_$snake',
      'operation': 'DELETE',
    };
    if (cascade) deleteMutation['cascade'] = true;
    mutations.add(deleteMutation);

    return {'queries': queries, 'mutations': mutations};
  }
}
