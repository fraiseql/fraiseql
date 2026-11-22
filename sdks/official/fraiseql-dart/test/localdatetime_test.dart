import 'dart:convert';

import 'package:fraiseql/fraiseql.dart';
import 'package:test/test.dart';

/// `FieldType.localDateTime` declares a date and time with no offset, a PostgreSQL
/// `timestamp`. Dart's `DateTime` is local or UTC by its value, not its type, so a date-time
/// is `DateTime` unless declared otherwise.
void main() {
  test('a localDateTime field and argument export as LocalDateTime', () {
    final schema = FraiseQLSchema()
      ..type('Shift', sqlSource: 'v_shift', fields: {
        'id': const FieldType.id(nullable: false),
        'startsAt': const FieldType.localDateTime(nullable: false),
        'endsAt': const FieldType.localDateTime(),
      })
      ..query('shifts',
          returnType: 'Shift',
          returnsList: true,
          sqlSource: 'v_shift',
          arguments: {
            'startsAfter': const FieldType.localDateTime(nullable: false)
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
    expect(fields['startsAt'], ('LocalDateTime', false));
    expect(fields['endsAt'], ('LocalDateTime', true));
    final argument = (((document['queries']! as List<Object?>).first!
            as Map<String, Object?>)['arguments']! as List<Object?>)
        .first! as Map<String, Object?>;
    expect(argument['type'], 'LocalDateTime');
  });
}
