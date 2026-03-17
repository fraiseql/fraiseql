import 'dart:async' as dart_async;
import 'dart:convert';

import 'package:http/http.dart' as http;

import 'errors.dart';

/// Configuration for a FraiseQL client.
class FraiseQLClientConfig {
  final String url;
  final String? authorization;
  final Future<String> Function()? authorizationFactory;
  final Duration timeout;
  final http.Client? httpClient;

  const FraiseQLClientConfig({
    required this.url,
    this.authorization,
    this.authorizationFactory,
    this.timeout = const Duration(seconds: 30),
    this.httpClient,
  });
}

/// FraiseQL HTTP client for Dart/Flutter.
class FraiseQLClient {
  final FraiseQLClientConfig _config;
  late final http.Client _http;
  bool _ownsClient = false;

  FraiseQLClient(FraiseQLClientConfig config) : _config = config {
    if (config.httpClient != null) {
      _http = config.httpClient!;
    } else {
      _http = http.Client();
      _ownsClient = true;
    }
  }

  FraiseQLClient.simple(String url)
      : this(FraiseQLClientConfig(url: url));

  /// Execute a GraphQL query.
  ///
  /// Returns the `data` portion of the response.
  /// Throws [GraphQLException] if the `errors` array is non-null and non-empty.
  /// Throws [NetworkException] on transport failure.
  Future<Map<String, Object?>> query(
    String query, {
    Map<String, Object?>? variables,
    String? operationName,
  }) =>
      _execute(query, variables: variables, operationName: operationName);

  /// Execute a GraphQL mutation.
  Future<Map<String, Object?>> mutate(
    String mutation, {
    Map<String, Object?>? variables,
    String? operationName,
  }) =>
      _execute(mutation, variables: variables, operationName: operationName);

  Future<Map<String, Object?>> _execute(
    String gqlQuery, {
    Map<String, Object?>? variables,
    String? operationName,
  }) async {
    final body = <String, Object?>{'query': gqlQuery};
    if (variables != null && variables.isNotEmpty) {
      body['variables'] = variables;
    }
    if (operationName != null) {
      body['operationName'] = operationName;
    }

    final headers = <String, String>{
      'Content-Type': 'application/json',
      'Accept': 'application/json',
    };

    // Resolve authorization header
    final auth = _config.authorizationFactory != null
        ? await _config.authorizationFactory!()
        : _config.authorization;
    if (auth != null) headers['Authorization'] = auth;

    final uri = Uri.parse(_config.url);

    http.Response response;
    try {
      response = await _http
          .post(uri, headers: headers, body: jsonEncode(body))
          .timeout(_config.timeout);
    } on dart_async.TimeoutException {
      throw TimeoutException(
        'Request timed out after ${_config.timeout.inSeconds}s',
      );
    } catch (e) {
      throw NetworkException('Request failed: $e', e);
    }

    switch (response.statusCode) {
      case 401:
      case 403:
        throw AuthenticationException(response.statusCode);
      case 429:
        throw const RateLimitException();
    }

    final Map<String, Object?> parsed;
    try {
      parsed = jsonDecode(response.body) as Map<String, Object?>;
    } catch (e) {
      throw NetworkException('Failed to parse response: $e', e);
    }

    // null errors = success (cross-SDK invariant)
    final errors = parsed['errors'];
    if (errors is List && errors.isNotEmpty) {
      final graphqlErrors = errors
          .whereType<Map<String, Object?>>()
          .map(GraphQLError.fromJson)
          .toList();
      throw GraphQLException(graphqlErrors);
    }

    final data = parsed['data'];
    return (data as Map<String, Object?>?) ?? {};
  }

  /// Close the HTTP client.
  void close() {
    if (_ownsClient) _http.close();
  }
}
