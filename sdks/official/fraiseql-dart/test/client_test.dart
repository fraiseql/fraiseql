import 'package:fraiseql/fraiseql.dart';
import 'package:test/test.dart';

void main() {
  group('FraiseQLClientConfig', () {
    test('constructs with required url', () {
      const config = FraiseQLClientConfig(url: 'http://localhost:8080/graphql');
      expect(config.url, equals('http://localhost:8080/graphql'));
      expect(config.authorization, isNull);
      expect(config.timeout, equals(const Duration(seconds: 30)));
    });

    test('constructs with optional fields', () {
      const config = FraiseQLClientConfig(
        url: 'http://localhost:8080/graphql',
        authorization: 'Bearer token123',
        timeout: Duration(seconds: 10),
      );
      expect(config.authorization, equals('Bearer token123'));
      expect(config.timeout, equals(const Duration(seconds: 10)));
    });
  });

  group('FraiseQLClient', () {
    test('simple constructor sets url', () {
      final client = FraiseQLClient.simple('http://localhost:8080/graphql');
      client.close();
    });

    test('close is idempotent for externally-owned http client', () {
      final client = FraiseQLClient.simple('http://localhost:8080/graphql');
      client.close();
      client.close(); // second close should not throw
    });
  });
}
