/// Marks a Dart class as a FraiseQL type for schema generation.
///
/// ```dart
/// @FraiseQLType()
/// class User {
///   @FraiseQLField()
///   final int id;
///
///   @FraiseQLField(description: "The user's display name")
///   final String name;
///
///   const User({required this.id, required this.name});
/// }
/// ```
class FraiseQLType {
  /// Optional override for the GraphQL type name. Defaults to the class name.
  final String? name;

  /// Optional description for the type.
  final String? description;

  /// The SQL view backing this type. Defaults to "v_" + snake_case(name).
  final String? sqlSource;

  /// When true, auto-generate CRUD queries and mutations.
  final bool crud;

  /// When true, generated CRUD mutations use GraphQL cascade.
  final bool cascade;

  const FraiseQLType({
    this.name,
    this.description,
    this.sqlSource,
    this.crud = false,
    this.cascade = false,
  });
}

/// Marks a field on a [FraiseQLType] class.
class FraiseQLField {
  /// Optional override for the GraphQL field name.
  final String? name;

  /// Whether the field is required (non-null in GraphQL). Default: true.
  final bool required;

  /// Optional description for the field.
  final String? description;

  /// Whether the field is deprecated.
  final bool deprecated;

  /// When true, this field is server-computed and excluded from CRUD input types.
  ///
  /// Computed fields (e.g. auto-generated slugs, view aggregations) are never
  /// provided by the client, so they are omitted from [Create{Type}Input] and
  /// [Update{Type}Input]. They remain visible in query results.
  final bool computed;

  const FraiseQLField({
    this.name,
    this.required = true,
    this.description,
    this.deprecated = false,
    this.computed = false,
  });
}
