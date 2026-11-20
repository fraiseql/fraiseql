import 'dart:convert';

import 'package:fraiseql/fraiseql.dart';
import 'package:test/test.dart';

/// `retryOnAbort` authoring (#1936): a mutation that opts out of transaction retry is
/// exported with `retry_on_abort: false`; one that does not declare it is exported without
/// the key, so it follows the server.
void main() {
  test('retry_on_abort is exported only when it is declared', () {
    final schema = FraiseQLSchema()
      ..mutation('chargeCard',
          returnType: 'Charge',
          sqlSource: 'fn_charge_card',
          retryOnAbort: false)
      ..mutation('refund', returnType: 'Charge', sqlSource: 'fn_refund');

    final document =
        jsonDecode(jsonEncode(schema.toJson())) as Map<String, Object?>;
    final mutations =
        (document['mutations']! as List<Object?>).cast<Map<String, Object?>>();

    expect(mutations[0]['retry_on_abort'], isFalse);
    expect(mutations[1].containsKey('retry_on_abort'), isFalse);
  });
}
