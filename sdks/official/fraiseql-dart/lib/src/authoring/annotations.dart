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

  const FraiseQLType({this.name, this.description});
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

  const FraiseQLField({
    this.name,
    this.required = true,
    this.description,
    this.deprecated = false,
  });
}
