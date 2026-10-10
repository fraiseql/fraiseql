import 'dart:convert';

import 'package:fraiseql/fraiseql.dart';
import 'package:test/test.dart';

/// `localized` authoring (#1527): a String field, input field or mutation argument stored
/// as a locale map. These tests follow the declaration to the exported JSON, and refuse it
/// on anything but a String where it is declared, as the Python SDK refuses
/// `Localized[int]`.
void main() {
  Map<String, Object?> exported(FraiseQLSchema schema) =>
      jsonDecode(jsonEncode(schema.toJson())) as Map<String, Object?>;

  Map<String, Object?> named(Object? items, String name) =>
      (items! as List<Object?>).cast<Map<String, Object?>>().firstWhere(
            (item) => item['name'] == name,
            orElse: () => fail('$name is absent from the exported schema'),
          );

  Map<String, Object?> member(Map<String, Object?> document, String section,
          String owner, String key, String name) =>
      named(named(document[section], owner)[key], name);

  test('a localized field and input field are exported as localized', () {
    final schema = FraiseQLSchema()
      ..type('Product', sqlSource: 'v_product', fields: {
        'id': const FieldType.id(nullable: false),
        'name': const FieldType.string(localized: true),
        'sku': const FieldType.string(nullable: false),
      })
      ..type('CreateProductInput', isInput: true, fields: {
        'name': const FieldType.string(nullable: false, localized: true),
      });

    final document = exported(schema);
    expect(member(document, 'types', 'Product', 'fields', 'name')['localized'],
        isTrue);
    expect(
        member(document, 'types', 'Product', 'fields', 'sku')
            .containsKey('localized'),
        isFalse);
    final inputs = <Object?>[
      ...(document['input_types'] as List<Object?>? ?? const []),
      ...(document['types']! as List<Object?>),
    ];
    expect(
        named(
            named(inputs, 'CreateProductInput')['fields'], 'name')['localized'],
        isTrue);
  });

  test('a localized mutation argument is exported as localized', () {
    final schema = FraiseQLSchema()
      ..mutation('createProduct',
          returnType: 'Product',
          sqlSource: 'fn_create_product',
          operation: 'insert',
          arguments: {
            'sku': const FieldType.string(nullable: false),
            'displayName': const FieldType.string(localized: true),
          });

    final document = exported(schema);
    expect(
        member(
            document, 'mutations', 'createProduct', 'arguments', 'displayName'),
        {
          'name': 'displayName',
          'type': 'String',
          'nullable': true,
          'localized': true,
        });
    expect(
        member(document, 'mutations', 'createProduct', 'arguments', 'sku')
            .containsKey('localized'),
        isFalse);
  });

  test('a CRUD input field is localized as its field is', () {
    final schema = FraiseQLSchema()
      ..type('Article', sqlSource: 'v_article', crud: true, fields: {
        'id': const FieldType.int_(nullable: false),
        'title': const FieldType.string(nullable: false, localized: true),
      });

    final document = exported(schema);
    final inputs = <Object?>[
      ...(document['input_types'] as List<Object?>? ?? const []),
      ...(document['types']! as List<Object?>),
    ];
    for (final input in ['CreateArticleInput', 'UpdateArticleInput']) {
      expect(
          named(named(inputs, input)['fields'], 'title')['localized'], isTrue,
          reason: input);
    }
  });

  test('localized is refused on a field that is not a String', () {
    expect(
      () => FraiseQLSchema().type('Priced', sqlSource: 'v_priced', fields: {
        'price':
            const FieldType.named('Float', nullable: false, localized: true),
      }),
      throwsA(isA<ArgumentError>().having(
          (e) => e.message,
          'message',
          allOf(
              contains('price'), contains('only a String can be localized')))),
    );
  });
}
