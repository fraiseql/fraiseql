// A Decimal travels as the string of its exact digits. The server takes a Decimal
// argument as a string such as "1.50", or an exact integer, and refuses a number with a
// fraction. Dart has no decimal type in its core library: a String keeps every digit both
// ways, and a double is sent as a JSON number, which the server refuses for a Decimal.

import 'dart:convert';

import 'package:fraiseql/fraiseql.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:test/test.dart';

/// A client whose server records the body it receives and echoes the variables back.
(FraiseQLClient, List<String>) echoClient() {
  final sent = <String>[];
  final mock = MockClient((request) async {
    sent.add(request.body);
    final variables =
        (jsonDecode(request.body) as Map<String, Object?>)['variables'];
    return http.Response(
      jsonEncode({
        'data': {'echo': variables},
      }),
      200,
      headers: {'content-type': 'application/json'},
    );
  });
  final client = FraiseQLClient(
    FraiseQLClientConfig(url: 'http://localhost/graphql', httpClient: mock),
  );
  return (client, sent);
}

void main() {
  group('Decimal values', () {
    for (final digits in [
      '1.50',
      '100.00',
      '12345678901234567890.12',
      '123456789012345678901234567890',
      '-0.00',
    ]) {
      test('a Decimal string $digits is sent and read as its exact digits',
          () async {
        final (client, sent) = echoClient();
        final data = await client.query(
          r'query($a: Decimal) { echo(a: $a) }',
          variables: {'a': digits},
        );
        expect(sent.single, contains('"a":"$digits"'));
        expect((data['echo'] as Map<String, Object?>)['a'], equals(digits));
      });
    }

    test('a double is sent as a number, which a Decimal argument refuses',
        () async {
      final (client, sent) = echoClient();
      await client.query(
        r'query($a: Decimal) { echo(a: $a) }',
        variables: {'a': 1.5},
      );
      expect(sent.single, contains('"a":1.5'));
    });
  });
}
