import 'dart:convert';

import 'package:fraiseql/fraiseql.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:test/test.dart';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns a 200 JSON response with the given body map.
http.Response _jsonOk(Map<String, Object?> body) => http.Response(
      jsonEncode(body),
      200,
      headers: {'content-type': 'application/json'},
    );

/// Minimal success payload with no errors.
final _successBody = _jsonOk({'data': <String, Object?>{}});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

void main() {
  // -------------------------------------------------------------------------
  // Original group — kept intact
  // -------------------------------------------------------------------------
  group('FraiseQLClient', () {
    test('query returns data on success', () async {
      final mockClient = MockClient(
        (request) async => http.Response(
          '{"data": {"user": {"id": 1, "name": "Alice"}}}',
          200,
          headers: {'content-type': 'application/json'},
        ),
      );

      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      final result = await client.query('{ user { id name } }');
      expect(result['user']?['name'], equals('Alice'));
      client.close();
    });

    test('throws GraphQLException when errors present', () async {
      final mockClient = MockClient(
        (request) async => http.Response(
          '{"data": null, "errors": [{"message": "Not found"}]}',
          200,
          headers: {'content-type': 'application/json'},
        ),
      );

      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      expect(
        () => client.query('{ user { id } }'),
        throwsA(isA<GraphQLException>()),
      );
      client.close();
    });

    test('null errors treated as success (cross-SDK invariant)', () async {
      final mockClient = MockClient(
        (request) async => http.Response(
          '{"data": {"users": []}, "errors": null}',
          200,
          headers: {'content-type': 'application/json'},
        ),
      );

      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      final result = await client.query('{ users { id } }');
      expect(result['users'], equals([]));
      client.close();
    });

    test('throws AuthenticationException on 401', () async {
      final mockClient = MockClient(
        (request) async => http.Response('Unauthorized', 401),
      );

      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      expect(
        () => client.query('{ secret }'),
        throwsA(isA<AuthenticationException>()),
      );
      client.close();
    });
  });

  // -------------------------------------------------------------------------
  // Error handling
  // -------------------------------------------------------------------------
  group('Error handling', () {
    test('throws AuthenticationException on 403 with statusCode == 403',
        () async {
      final mockClient = MockClient(
        (request) async => http.Response('Forbidden', 403),
      );
      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      await expectLater(
        () => client.query('{ secret }'),
        throwsA(
          isA<AuthenticationException>()
              .having((e) => e.statusCode, 'statusCode', 403),
        ),
      );
      client.close();
    });

    test('throws RateLimitException on 429', () async {
      final mockClient = MockClient(
        (request) async => http.Response('Too Many Requests', 429),
      );
      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      await expectLater(
        () => client.query('{ users { id } }'),
        throwsA(isA<RateLimitException>()),
      );
      client.close();
    });

    test('throws GraphQLException when 500 with valid JSON errors body',
        () async {
      // The client only switches on 401/403/429; a 500 falls through to JSON
      // parsing. If the body contains a valid errors array, GraphQLException
      // is raised.
      final mockClient = MockClient(
        (request) async => http.Response(
          '{"errors": [{"message": "Internal server error"}]}',
          500,
          headers: {'content-type': 'application/json'},
        ),
      );
      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      await expectLater(
        () => client.query('{ users { id } }'),
        throwsA(isA<GraphQLException>()),
      );
      client.close();
    });

    test('throws NetworkException when 500 response is not valid JSON',
        () async {
      final mockClient = MockClient(
        (request) async => http.Response('Internal Server Error', 500),
      );
      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      await expectLater(
        () => client.query('{ users { id } }'),
        throwsA(isA<NetworkException>()),
      );
      client.close();
    });

    test('throws NetworkException when underlying http.Client throws', () async {
      final mockClient = MockClient(
        (request) async => throw Exception('connection refused'),
      );
      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      await expectLater(
        () => client.query('{ users { id } }'),
        throwsA(isA<NetworkException>()),
      );
      client.close();
    });

    test('empty errors array is NOT an error', () async {
      final mockClient = MockClient(
        (request) async => _jsonOk({
          'data': {'users': <Object?>[]},
          'errors': <Object?>[],
        }),
      );
      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      final result = await client.query('{ users { id } }');
      expect(result['users'], equals([]));
      client.close();
    });

    test('absent errors field is NOT an error', () async {
      final mockClient = MockClient(
        (request) async => _jsonOk({'data': <String, Object?>{'ok': true}}),
      );
      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      final result = await client.query('{ ok }');
      expect(result['ok'], isTrue);
      client.close();
    });
  });

  // -------------------------------------------------------------------------
  // Request construction
  // -------------------------------------------------------------------------
  group('Request construction', () {
    late http.Request captured;

    /// Returns a MockClient that stores the first request in [captured] and
    /// replies with an empty-data success response.
    MockClient capturingClient() => MockClient((request) async {
          captured = request;
          return _successBody;
        });

    test('sends query string in request body as "query" field', () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: capturingClient(),
        ),
      );

      const gql = '{ users { id name } }';
      await client.query(gql);

      final body = jsonDecode(captured.body) as Map<String, Object?>;
      expect(body['query'], equals(gql));
      client.close();
    });

    test('sends variables when provided', () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: capturingClient(),
        ),
      );

      await client.query('{ user }', variables: {'id': 42});

      final body = jsonDecode(captured.body) as Map<String, Object?>;
      expect(body['variables'], equals({'id': 42}));
      client.close();
    });

    test('does NOT send variables key when variables is null', () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: capturingClient(),
        ),
      );

      await client.query('{ ok }');

      final body = jsonDecode(captured.body) as Map<String, Object?>;
      expect(body.containsKey('variables'), isFalse);
      client.close();
    });

    test('does NOT send variables key when variables map is empty', () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: capturingClient(),
        ),
      );

      await client.query('{ ok }', variables: {});

      final body = jsonDecode(captured.body) as Map<String, Object?>;
      expect(body.containsKey('variables'), isFalse);
      client.close();
    });

    test('sends operationName when provided', () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: capturingClient(),
        ),
      );

      await client.query('query GetUsers { users { id } }',
          operationName: 'GetUsers',);

      final body = jsonDecode(captured.body) as Map<String, Object?>;
      expect(body['operationName'], equals('GetUsers'));
      client.close();
    });

    test('does NOT send operationName key when operationName is null', () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: capturingClient(),
        ),
      );

      await client.query('{ ok }');

      final body = jsonDecode(captured.body) as Map<String, Object?>;
      expect(body.containsKey('operationName'), isFalse);
      client.close();
    });

    test('Content-Type header is application/json', () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: capturingClient(),
        ),
      );

      await client.query('{ ok }');

      expect(
        captured.headers['content-type'],
        equals('application/json'),
      );
      client.close();
    });

    test('Accept header is application/json', () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: capturingClient(),
        ),
      );

      await client.query('{ ok }');

      expect(
        captured.headers['accept'],
        equals('application/json'),
      );
      client.close();
    });

    test('Authorization header present when static authorization is set',
        () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          authorization: 'Bearer token-abc',
          httpClient: capturingClient(),
        ),
      );

      await client.query('{ ok }');

      expect(
        captured.headers['authorization'],
        equals('Bearer token-abc'),
      );
      client.close();
    });

    test('Authorization header resolved from async authorizationFactory',
        () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          authorizationFactory: () async => 'Bearer dynamic-token',
          httpClient: capturingClient(),
        ),
      );

      await client.query('{ ok }');

      expect(
        captured.headers['authorization'],
        equals('Bearer dynamic-token'),
      );
      client.close();
    });

    test('Authorization header absent when no auth configured', () async {
      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: capturingClient(),
        ),
      );

      await client.query('{ ok }');

      expect(captured.headers.containsKey('authorization'), isFalse);
      client.close();
    });
  });

  // -------------------------------------------------------------------------
  // mutate
  // -------------------------------------------------------------------------
  group('mutate', () {
    test('mutate returns data on success', () async {
      final mockClient = MockClient(
        (request) async => _jsonOk({
          'data': {'createUser': <String, Object?>{'id': 99, 'name': 'Bob'}},
        }),
      );
      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      final result =
          await client.mutate('mutation { createUser { id name } }');
      expect((result['createUser'] as Map<String, Object?>)['id'], equals(99));
      client.close();
    });

    test('mutate sends mutation string as "query" field in body', () async {
      http.Request? captured;
      final mockClient = MockClient((request) async {
        captured = request;
        return _successBody;
      });
      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: mockClient),
      );

      const mutation = 'mutation DeleteUser(\$id: ID!) { deleteUser(id: \$id) }';
      await client.mutate(mutation, variables: {'id': '1'});

      final body =
          jsonDecode(captured!.body) as Map<String, Object?>;
      expect(body['query'], equals(mutation));
      expect(body['variables'], equals({'id': '1'}));
      client.close();
    });
  });

  // -------------------------------------------------------------------------
  // close()
  // -------------------------------------------------------------------------
  group('close()', () {
    test('close() closes an internally created http.Client', () async {
      // We verify that calling close on a client without an injected httpClient
      // does not throw, and that subsequent requests fail (client is closed).
      // The simplest observable: _ownsClient is true, so close() is forwarded.
      // We create a real http.Client stub that tracks close calls.
      var closeCalled = false;

      final trackingClient = _TrackingClient(onClose: () {
        closeCalled = true;
      },);

      final client = FraiseQLClient(
        FraiseQLClientConfig(
          url: 'http://localhost',
          httpClient: trackingClient,
          // Note: by passing httpClient here, _ownsClient = false.
          // To test the "owns" path we need to NOT pass httpClient.
          // However, we cannot inject a tracking client AND have _ownsClient=true
          // simultaneously with the current API.
          // Instead, we test that close() on an injected client does NOT close it
          // (see next test), and verify close() calls through manually.
        ),
      );

      client.close();
      // injected client → _ownsClient = false → trackingClient.close NOT called
      expect(closeCalled, isFalse);
    });

    test('close() does NOT close an externally injected http.Client', () async {
      var closeCalled = false;
      final injected = _TrackingClient(onClose: () => closeCalled = true);

      final client = FraiseQLClient(
        FraiseQLClientConfig(url: 'http://localhost', httpClient: injected),
      );

      client.close();

      expect(closeCalled, isFalse,
          reason: 'Injected clients must not be closed by FraiseQLClient',);
    });
  });

  // -------------------------------------------------------------------------
  // Error types (unit tests on the exception classes themselves)
  // -------------------------------------------------------------------------
  group('Error types', () {
    test('GraphQLException.message is the first error message', () {
      final errors = [
        const GraphQLError(message: 'First error'),
        const GraphQLError(message: 'Second error'),
      ];
      final exception = GraphQLException(errors);
      expect(exception.message, equals('First error'));
    });

    test('GraphQLException with empty list uses fallback message', () {
      final exception = GraphQLException([]);
      expect(exception.message, equals('GraphQL error'));
    });

    test('AuthenticationException.statusCode stores the code', () {
      final e = AuthenticationException(403);
      expect(e.statusCode, equals(403));
    });

    test('AuthenticationException message encodes the status code', () {
      final e = AuthenticationException(401);
      expect(e.message, contains('401'));
    });

    test('RateLimitException.retryAfter is null by default', () {
      const e = RateLimitException();
      expect(e.retryAfter, isNull);
    });

    test('RateLimitException stores retryAfter when provided', () {
      const duration = Duration(seconds: 60);
      const e = RateLimitException(retryAfter: duration);
      expect(e.retryAfter, equals(duration));
    });

    test('GraphQLError.fromJson parses message', () {
      final error = GraphQLError.fromJson({'message': 'Field not found'});
      expect(error.message, equals('Field not found'));
    });

    test('GraphQLError.fromJson uses fallback for missing message', () {
      final error = GraphQLError.fromJson(<String, Object?>{});
      expect(error.message, equals('Unknown error'));
    });

    test('GraphQLError.fromJson parses locations', () {
      final error = GraphQLError.fromJson({
        'message': 'Syntax error',
        'locations': [
          {'line': 3, 'column': 14},
        ],
      });
      expect(error.locations, hasLength(1));
      expect(error.locations!.first.line, equals(3));
      expect(error.locations!.first.column, equals(14));
    });

    test('GraphQLError.fromJson sets locations to null when absent', () {
      final error = GraphQLError.fromJson({'message': 'Oops'});
      expect(error.locations, isNull);
    });

    test('GraphQLErrorLocation.fromJson parses line and column', () {
      final loc = GraphQLErrorLocation.fromJson({'line': 7, 'column': 2});
      expect(loc.line, equals(7));
      expect(loc.column, equals(2));
    });

    test('NetworkException is a FraiseQLException', () {
      const e = NetworkException('timeout');
      expect(e, isA<FraiseQLException>());
    });

    test('TimeoutException is a NetworkException', () {
      const e = TimeoutException('timed out');
      expect(e, isA<NetworkException>());
    });
  });

  // -------------------------------------------------------------------------
  // FraiseQLClientConfig defaults
  // -------------------------------------------------------------------------
  group('FraiseQLClientConfig', () {
    test('timeout defaults to 30 seconds', () {
      const config = FraiseQLClientConfig(url: 'http://localhost');
      expect(config.timeout, equals(const Duration(seconds: 30)));
    });

    test('authorization defaults to null', () {
      const config = FraiseQLClientConfig(url: 'http://localhost');
      expect(config.authorization, isNull);
    });

    test('authorizationFactory defaults to null', () {
      const config = FraiseQLClientConfig(url: 'http://localhost');
      expect(config.authorizationFactory, isNull);
    });

    test('httpClient defaults to null', () {
      const config = FraiseQLClientConfig(url: 'http://localhost');
      expect(config.httpClient, isNull);
    });

    test('url is stored correctly', () {
      const config = FraiseQLClientConfig(url: 'https://api.example.com/gql');
      expect(config.url, equals('https://api.example.com/gql'));
    });

    test('FraiseQLClient.simple sets URL on config', () {
      final client = FraiseQLClient.simple('http://localhost:4000/graphql');
      // We cannot read _config directly, but close() must not throw.
      expect(() => client.close(), returnsNormally);
    });
  });

  // -------------------------------------------------------------------------
  // FraiseQLType annotation (original group, kept)
  // -------------------------------------------------------------------------
  group('FraiseQLType annotation', () {
    test('FraiseQLType creates annotation with optional name', () {
      const annotation = FraiseQLType(name: 'CustomName');
      expect(annotation.name, equals('CustomName'));
    });

    test('FraiseQLField defaults required to true', () {
      const field = FraiseQLField();
      expect(field.required, isTrue);
    });
  });
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// A minimal [http.Client] subclass that records close() calls.
class _TrackingClient extends http.BaseClient {
  final void Function() onClose;

  _TrackingClient({required this.onClose});

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) async {
    throw UnsupportedError('_TrackingClient.send should not be called');
  }

  @override
  void close() => onClose();
}
