import 'dart:convert';

import 'package:http/http.dart' as http;

import 'errors.dart';

/// Configuration for a [FraiseQLClient].
class FraiseQLClientConfig {
  /// The GraphQL endpoint URL.
  final String url;

  /// A static authorization header value (e.g. `'Bearer xxx'`).
  final String? authorization;

  /// An async factory that returns an authorization header value.
  ///
  /// Called on every request, allowing dynamic token refresh.
  final Future<String> Function()? authorizationFactory;

  /// An optional [http.Client] to use for requests.
  ///
  /// When provided, the client will **not** be closed by [FraiseQLClient.close].
  final http.Client? httpClient;

  /// The request timeout. Defaults to 30 seconds.
  final Duration timeout;

  /// Creates a [FraiseQLClientConfig].
  const FraiseQLClientConfig({
    required this.url,
    this.authorization,
    this.authorizationFactory,
    this.httpClient,
    this.timeout = const Duration(seconds: 30),
  });
}

/// A GraphQL client for communicating with a FraiseQL server.
class FraiseQLClient {
  final FraiseQLClientConfig _config;
  final http.Client _httpClient;
  final bool _ownsClient;

  /// Creates a [FraiseQLClient] from the given [config].
  FraiseQLClient(FraiseQLClientConfig config)
      : _config = config,
        _httpClient = config.httpClient ?? http.Client(),
        _ownsClient = config.httpClient == null;

  /// Creates a [FraiseQLClient] with a minimal configuration for [url].
  FraiseQLClient.simple(String url)
      : this(FraiseQLClientConfig(url: url));

  /// Executes a GraphQL query and returns the `data` map from the response.
  ///
  /// Throws [GraphQLException] if the response contains GraphQL errors,
  /// [AuthenticationException] on 401/403, [RateLimitException] on 429,
  /// or [NetworkException] on other failures.
  Future<Map<String, dynamic>> query(
    String query, {
    Map<String, Object?>? variables,
    String? operationName,
  }) =>
      _execute(query, variables: variables, operationName: operationName);

  /// Executes a GraphQL mutation and returns the `data` map from the response.
  ///
  /// Behaves identically to [query]; the separate method exists for semantic
  /// clarity.
  Future<Map<String, dynamic>> mutate(
    String mutation, {
    Map<String, Object?>? variables,
    String? operationName,
  }) =>
      _execute(mutation, variables: variables, operationName: operationName);

  /// Closes the underlying HTTP client.
  ///
  /// If the client was injected via [FraiseQLClientConfig.httpClient], this
  /// method is a no-op (the caller retains ownership).
  void close() {
    if (_ownsClient) {
      _httpClient.close();
    }
  }

  // ---------------------------------------------------------------------------
  // Internal
  // ---------------------------------------------------------------------------

  Future<Map<String, dynamic>> _execute(
    String query, {
    Map<String, Object?>? variables,
    String? operationName,
  }) async {
    final body = <String, Object?>{'query': query};
    if (variables != null && variables.isNotEmpty) {
      body['variables'] = variables;
    }
    if (operationName != null) {
      body['operationName'] = operationName;
    }

    final http.Response response;
    try {
      final request = http.Request('POST', Uri.parse(_config.url));
      request.headers['content-type'] = 'application/json';
      request.headers['accept'] = 'application/json';

      // Authorization
      final auth = _config.authorization ??
          (await _config.authorizationFactory?.call());
      if (auth != null) {
        request.headers['authorization'] = auth;
      }

      request.body = jsonEncode(body);

      final streamed = await _httpClient.send(request);
      response = await http.Response.fromStream(streamed);
    } on FraiseQLException {
      rethrow;
    } catch (e) {
      throw NetworkException(e.toString());
    }

    final statusCode = response.statusCode;

    // Authentication errors
    if (statusCode == 401 || statusCode == 403) {
      throw AuthenticationException(statusCode);
    }

    // Rate limiting
    if (statusCode == 429) {
      throw const RateLimitException();
    }

    // Attempt to parse JSON
    final Map<String, Object?> json;
    try {
      json = jsonDecode(response.body) as Map<String, Object?>;
    } catch (_) {
      throw NetworkException(
        'Unexpected response (status $statusCode): ${response.body}',
      );
    }

    // Check for GraphQL errors
    final rawErrors = json['errors'] as List<Object?>?;
    if (rawErrors != null && rawErrors.isNotEmpty) {
      final errors = rawErrors
          .map((e) => GraphQLError.fromJson(e! as Map<String, Object?>))
          .toList();
      throw GraphQLException(errors);
    }

    return (json['data'] as Map<String, dynamic>?) ?? <String, dynamic>{};
  }
}
