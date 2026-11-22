import 'dart:convert';

import 'package:fraiseql/fraiseql.dart';
import 'package:test/test.dart';

/// `FieldType.bigInt` declares a signed 64-bit integer, served as the string of its digits.
/// Dart's `int` is the type for small values, so `FieldType.int_` stays `Int`, which is 32
/// bits and refuses a value past that at read time.
void main() {
  test('a bigInt field and argument export as BigInt, and int_ stays Int', () {
    final schema = FraiseQLSchema()
      ..type('Ledger', sqlSource: 'v_ledger', fields: {
        'id': const FieldType.id(nullable: false),
        'amount': const FieldType.bigInt(nullable: false),
        'balance': const FieldType.bigInt(),
        'count': const FieldType.int_(nullable: false),
      })
      ..query('ledgers',
          returnType: 'Ledger',
          returnsList: true,
          sqlSource: 'v_ledger',
          arguments: {
            'accountNumber': const FieldType.bigInt(nullable: false)
          });

    final document =
        jsonDecode(jsonEncode(schema.toJson())) as Map<String, Object?>;
    final fields = {
      for (final f in ((document['types']! as List<Object?>).first!
          as Map<String, Object?>)['fields']! as List<Object?>)
        (f! as Map<String, Object?>)['name']: (
          (f as Map<String, Object?>)['type'],
          f['nullable']
        ),
    };
    expect(fields['amount'], ('BigInt', false));
    expect(fields['balance'], ('BigInt', true));
    expect(fields['count'], ('Int', false));
    final argument = (((document['queries']! as List<Object?>).first!
            as Map<String, Object?>)['arguments']! as List<Object?>)
        .first! as Map<String, Object?>;
    expect(argument['type'], 'BigInt');
  });
}
