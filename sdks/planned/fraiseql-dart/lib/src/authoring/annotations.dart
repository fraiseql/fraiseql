/// Schema authoring annotations for FraiseQL.
library;

/// The current SDK version.
const String version = '0.0.0';

/// Marks a class as a FraiseQL GraphQL type.
///
/// ```dart
/// @FraiseQLType(name: 'User')
/// class UserModel {
///   // ...
/// }
/// ```
class FraiseQLType {
  /// An optional custom name for the GraphQL type.
  ///
  /// When omitted, the annotated class name is used.
  final String? name;

  /// Creates a [FraiseQLType] annotation.
  const FraiseQLType({this.name});
}

/// Marks a field within a [FraiseQLType]-annotated class.
class FraiseQLField {
  /// Whether the field is required (non-nullable) in the GraphQL schema.
  final bool required;

  /// Creates a [FraiseQLField] annotation.
  const FraiseQLField({this.required = true});
}
