/// Base class for all FraiseQL SDK exceptions.
class FraiseQLException implements Exception {
  final String message;
  final Object? cause;

  const FraiseQLException(this.message, [this.cause]);

  @override
  String toString() => 'FraiseQLException: $message';
}

/// One or more errors from the GraphQL errors array.
class GraphQLException extends FraiseQLException {
  final List<GraphQLError> errors;

  GraphQLException(this.errors)
      : super(errors.isEmpty ? 'GraphQL error' : errors.first.message);
}

/// Transport-level error.
class NetworkException extends FraiseQLException {
  const NetworkException(super.message, [super.cause]);
}

/// Request timeout error.
class TimeoutException extends NetworkException {
  const TimeoutException([super.message = 'Request timed out']);
}

/// Authentication error (401/403).
class AuthenticationException extends FraiseQLException {
  final int statusCode;
  AuthenticationException(this.statusCode)
      : super('Authentication failed (HTTP $statusCode)');
}

/// Rate limit error (429).
class RateLimitException extends FraiseQLException {
  final Duration? retryAfter;
  const RateLimitException({this.retryAfter}) : super('Rate limit exceeded');
}

/// Immutable GraphQL error entry from the response errors array.
class GraphQLError {
  final String message;
  final List<GraphQLErrorLocation>? locations;
  final List<Object>? path;
  final Map<String, Object?>? extensions;

  const GraphQLError({
    required this.message,
    this.locations,
    this.path,
    this.extensions,
  });

  factory GraphQLError.fromJson(Map<String, Object?> json) => GraphQLError(
        message: json['message'] as String? ?? 'Unknown error',
        locations: (json['locations'] as List<Object?>?)
            ?.map(
              (l) => GraphQLErrorLocation.fromJson(l as Map<String, Object?>),
            )
            .toList(),
      );
}

/// Line/column location in a GraphQL document.
class GraphQLErrorLocation {
  final int line;
  final int column;

  const GraphQLErrorLocation({required this.line, required this.column});

  factory GraphQLErrorLocation.fromJson(Map<String, Object?> json) =>
      GraphQLErrorLocation(
        line: (json['line'] as num).toInt(),
        column: (json['column'] as num).toInt(),
      );
}
