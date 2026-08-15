import 'dart:convert';

import 'package:fraiseql/fraiseql.dart';
import 'package:test/test.dart';

/// pgvector field authoring: `vector_config` and `vector_distance` (#959).
///
/// The compiler refuses a `Vector` field carrying no `vector_config`, so an SDK that
/// cannot author the config cannot author the type at all. These tests follow the
/// declaration all the way to the exported JSON, because this SDK shipped with no
/// export path at all while a hand-written parity generator reported success (#853) —
/// the whole class of "authored, then lost".
void main() {
  Map<String, Map<String, Object?>> documentFields() {
    final schema = FraiseQLSchema();
    schema.type(
      'Document',
      sqlSource: 'v_document',
      fields: {
        'id': const FieldType.id(nullable: false),
        'embedding': const FieldType.vector(
          'Vector',
          VectorConfig(1536,
              indexType: VectorConfig.indexIvfFlat,
              distanceMetric: VectorConfig.metricL2),
          nullable: false,
        ),
        'fingerprint': const FieldType.vector(
          'BitVector',
          VectorConfig(768, distanceMetric: VectorConfig.metricHamming),
          nullable: false,
        ),
        'compact': const FieldType.vector(
          'HalfVector',
          VectorConfig(1536, distanceMetric: VectorConfig.metricInnerProduct),
        ),
        'terms': const FieldType.vector(
          'SparseVector',
          VectorConfig(30000, indexType: VectorConfig.indexNone),
        ),
        'plain':
            const FieldType.vector('Vector', VectorConfig(8), nullable: false),
        'similarity':
            const FieldType.vectorDistanceOf('embedding', nullable: false),
      },
    );

    final decoded =
        jsonDecode(jsonEncode(schema.toJson())) as Map<String, Object?>;
    final types = decoded['types']! as List<Object?>;
    final document = types.cast<Map<String, Object?>>().firstWhere(
          (t) => t['name'] == 'Document',
        );
    final fields =
        (document['fields']! as List<Object?>).cast<Map<String, Object?>>();
    return {for (final f in fields) f['name']! as String: f};
  }

  test('the four pgvector field types keep their own type name', () {
    final fields = documentFields();
    expect(fields['embedding']!['type'], equals('Vector'));
    expect(fields['fingerprint']!['type'], equals('BitVector'));
    expect(fields['compact']!['type'], equals('HalfVector'));
    expect(fields['terms']!['type'], equals('SparseVector'));
  });

  test('every key of vector_config survives to schema.json', () {
    // Every key is asserted, not just the object's presence: index_type and
    // distance_metric both have compiler-side defaults, so a config that lost them
    // would still compile — to hnsw + cosine, chosen by nobody.
    final fields = documentFields();
    expect(
      fields['embedding']!['vector_config'],
      equals({
        'dimensions': 1536,
        'index_type': 'ivf_flat',
        'distance_metric': 'l2'
      }),
    );
    expect(
      (fields['fingerprint']!['vector_config']! as Map)['distance_metric'],
      equals('hamming'),
    );
    expect(
      (fields['compact']!['vector_config']! as Map)['distance_metric'],
      equals('inner_product'),
    );
    expect((fields['terms']!['vector_config']! as Map)['index_type'],
        equals('none'));
  });

  test('the index and metric left to the default are written out', () {
    expect(
      documentFields()['plain']!['vector_config'],
      equals(
          {'dimensions': 8, 'index_type': 'hnsw', 'distance_metric': 'cosine'}),
    );
  });

  test('a distance field names the vector it measures', () {
    expect(documentFields()['similarity']!['vector_distance'],
        equals('embedding'));
  });

  test('an ordinary field carries no vector keys', () {
    final id = documentFields()['id']!;
    expect(id.containsKey('vector_config'), isFalse);
    expect(id.containsKey('vector_distance'), isFalse);
  });

  test('a field is an embedding or a distance, not both', () {
    expect(
      () => FieldType.named('Vector',
          vectorConfig: const VectorConfig(8), vectorDistance: 'embedding'),
      throwsA(isA<AssertionError>()),
    );
  });

  test('a dimension count no column can have is refused', () {
    expect(() => VectorConfig(0), throwsA(isA<AssertionError>()));
  });
}
