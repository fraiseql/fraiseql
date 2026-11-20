import 'dart:convert';

import 'package:fraiseql/fraiseql.dart';
import 'package:test/test.dart';

/// `scopedLikeParent` authoring: a field that embeds a scoped type is exported with
/// `scoped_like_parent: true`; every other field is exported without the key. The option
/// is refused where it has no meaning: on an input type's field and on an argument.
void main() {
  Map<String, Object?> field(
          Map<String, Object?> document, String type, String name) =>
      ((document['types']! as List<Object?>)
              .cast<Map<String, Object?>>()
              .firstWhere((t) => t['name'] == type)['fields']! as List<Object?>)
          .cast<Map<String, Object?>>()
          .firstWhere((f) => f['name'] == name);

  test('scoped_like_parent is exported only on the field that declares it', () {
    final schema = FraiseQLSchema()
      ..type('Order', sqlSource: 'v_order', fields: {
        'id': const FieldType.id(nullable: false),
      })
      ..type('OrderShipment', sqlSource: 'v_order_shipment', fields: {
        'id': const FieldType.id(nullable: false),
        'order': const FieldType.named('Order',
            nullable: false, scopedLikeParent: true),
        'previous': const FieldType.named('Order', scopedLikeParent: false),
      });

    final document =
        jsonDecode(jsonEncode(schema.toJson())) as Map<String, Object?>;

    expect(field(document, 'OrderShipment', 'order'), {
      'name': 'order',
      'type': 'Order',
      'nullable': false,
      'scoped_like_parent': true,
    });
    for (final (type, name) in [
      ('OrderShipment', 'id'),
      ('OrderShipment', 'previous'),
      ('Order', 'id'),
    ]) {
      final keys = field(document, type, name).keys;
      expect(keys, isNot(contains('scoped_like_parent')),
          reason: '$type.$name');
    }
  });

  const scoped = FieldType.named('Order', scopedLikeParent: true);
  final refusal = throwsA(isA<ArgumentError>().having((e) => e.message,
      'message', allOf(contains("'order'"), contains('scopedLikeParent'))));

  test('scopedLikeParent is refused on an input field', () {
    expect(
      () => FraiseQLSchema()
          .type('ShipmentInput', isInput: true, fields: {'order': scoped}),
      refusal,
    );
  });

  test('scopedLikeParent is refused on an argument', () {
    expect(
      () => FraiseQLSchema().query('shipments',
          returnType: 'OrderShipment',
          sqlSource: 'v_order_shipment',
          arguments: {'order': scoped}),
      refusal,
    );
    expect(
      () => FraiseQLSchema().mutation('ship',
          returnType: 'OrderShipment',
          sqlSource: 'fn_ship',
          arguments: {'order': scoped}),
      refusal,
    );
  });
}
