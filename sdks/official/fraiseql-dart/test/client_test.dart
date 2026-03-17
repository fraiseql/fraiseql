import 'package:fraiseql/fraiseql.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:test/test.dart';

void main() {
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
