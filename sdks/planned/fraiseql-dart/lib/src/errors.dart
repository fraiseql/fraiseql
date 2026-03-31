/// Exception and error types for the FraiseQL Dart SDK.

/// Base exception for all FraiseQL errors.
class FraiseQLException implements Exception {
  /// A human-readable description of the error.
  final String message;

  /// Creates a [FraiseQLException] with the given [message].
  const FraiseQLException(this.message);

  @override
  String toString() => 'FraiseQLException: $message';
}

/// Represents a single GraphQL error returned by the server.
class GraphQLError {
  /// The error message.
  final String message;

  /// Optional source locations associated with the error.
  final List<GraphQLErrorLocation>? locations;

  /// Creates a [GraphQLError].
  const GraphQLError({required this.message, this.locations});

  /// Parses a [GraphQLError] from a JSON map.
  ///
  /// Falls back to `'Unknown error'` when the `message` key is absent.
  factory GraphQLError.fromJson(Map<String, Object?> json) {
    final message = json['message'] as String? ?? 'Unknown error';
    final rawLocations = json['locations'] as List<Object?>?;
    final locations = rawLocations
        ?.map((loc) =>
            GraphQLErrorLocation.fromJson(loc! as Map<String, Object?>))
        .toList();
    return GraphQLError(message: message, locations: locations);
  }

  @override
  String toString() => 'GraphQLError: $message';
}

/// A source location within a GraphQL document.
class GraphQLErrorLocation {
  /// The line number (1-based).
  final int line;

  /// The column number (1-based).
  final int column;

  /// Creates a [GraphQLErrorLocation].
  const GraphQLErrorLocation({required this.line, required this.column});

  /// Parses a [GraphQLErrorLocation] from a JSON map.
  factory GraphQLErrorLocation.fromJson(Map<String, Object?> json) {
    return GraphQLErrorLocation(
      line: json['line']! as int,
      column: json['column']! as int,
    );
  }

  @override
  String toString() => 'GraphQLErrorLocation(line: $line, column: $column)';
}

/// Thrown when the server returns one or more GraphQL errors.
class GraphQLException extends FraiseQLException {
  /// The list of [GraphQLError]s returned by the server.
  final List<GraphQLError> errors;

  /// Creates a [GraphQLException].
  ///
  /// The [message] is derived from the first error, or falls back to
  /// `'GraphQL error'` when the list is empty.
  GraphQLException(this.errors)
      : super(errors.isNotEmpty ? errors.first.message : 'GraphQL error');

  @override
  String toString() => 'GraphQLException: $message';
}

/// Thrown when the server responds with 401 or 403.
class AuthenticationException extends FraiseQLException {
  /// The HTTP status code that triggered this exception.
  final int statusCode;

  /// Creates an [AuthenticationException] for the given [statusCode].
  AuthenticationException(this.statusCode)
      : super('Authentication failed with status $statusCode');

  @override
  String toString() => 'AuthenticationException: $message';
}

/// Thrown when the server responds with 429 (Too Many Requests).
class RateLimitException extends FraiseQLException {
  /// Optional duration after which the request may be retried.
  final Duration? retryAfter;

  /// Creates a [RateLimitException].
  const RateLimitException({this.retryAfter})
      : super('Rate limit exceeded');

  @override
  String toString() => 'RateLimitException: $message';
}

/// Thrown when a network-level error occurs (connection failure, invalid
/// response body, etc.).
class NetworkException extends FraiseQLException {
  /// Creates a [NetworkException] with the given [message].
  const NetworkException(super.message);

  @override
  String toString() => 'NetworkException: $message';
}

/// Thrown when a request exceeds the configured timeout.
class TimeoutException extends NetworkException {
  /// Creates a [TimeoutException] with the given [message].
  const TimeoutException(super.message);

  @override
  String toString() => 'TimeoutException: $message';
}
