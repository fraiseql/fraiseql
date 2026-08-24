// Authors the cross-SDK conformance fixture with the Dart SDK's public API.
//
// Driven by `sdks/official/conformance/run.py`; see
// `sdks/official/conformance/README.md`.
//
// The one rule for every SDK's copy of this file: author through the SDK, never
// hand-assemble the JSON. The pre-existing `generate_parity_schema_test.dart` built the
// expected document as a literal map and never imported the package, so it passed while
// the package had no schema exporter at all (#853).

import 'dart:io';

import 'package:fraiseql/fraiseql.dart';

FraiseQLSchema authorMinimal() {
  final schema = FraiseQLSchema();

  schema.type(
    'User',
    sqlSource: 'v_user',
    fields: {
      'id': const FieldType.id(nullable: false),
      'email': const FieldType.string(nullable: false),
    },
  );

  schema.query(
    'users',
    returnType: 'User',
    returnsList: true,
    nullable: false,
    sqlSource: 'v_user',
  );

  return schema;
}

FraiseQLSchema authorFull() {
  final schema = FraiseQLSchema();

  schema.type(
    'User',
    sqlSource: 'v_user',
    relay: true,
    fields: {
      'id': const FieldType.id(nullable: false),
      'email': const FieldType.string(nullable: false),
      'name': const FieldType.string(
        description: 'The user\'s "display" name',
        deprecated: 'use displayName',
      ),
      'salary': const FieldType.float(requiresScope: 'read:User.salary'),
    },
  );

  schema.type(
    'Order',
    sqlSource: 'v_order',
    fields: {
      'id': const FieldType.id(nullable: false),
      'total': const FieldType.float(nullable: false),
      'status': const FieldType.string(nullable: false),
    },
  );

  schema.type(
    'UserNotFound',
    sqlSource: 'v_user_not_found',
    isError: true,
    fields: {
      'message': const FieldType.string(nullable: false),
      'code': const FieldType.string(nullable: false),
    },
  );

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
      'similarity':
          const FieldType.vectorDistanceOf('embedding', nullable: false),
    },
  );

  schema.type(
    'CreateUserInput',
    isInput: true,
    fields: {
      'email': const FieldType.string(nullable: false),
      'name': const FieldType.string(),
    },
  );

  schema.enumType('OrderStatus', ['PENDING', 'SHIPPED', 'CANCELLED']);

  schema.query(
    'users',
    returnType: 'User',
    returnsList: true,
    nullable: false,
    sqlSource: 'v_user',
  );

  schema.query(
    'user',
    returnType: 'User',
    returnsList: false,
    nullable: true,
    sqlSource: 'v_user',
    arguments: {'id': const FieldType.id(nullable: false)},
  );

  schema.query(
    'tenantOrders',
    returnType: 'Order',
    returnsList: true,
    nullable: false,
    sqlSource: 'v_order',
    inject: {'tenant_id': 'jwt:tenant_id'},
    cacheTtlSeconds: 300,
    requiresRole: 'admin',
  );

  schema.mutation(
    'createUser',
    returnType: 'User',
    sqlSource: 'fn_create_user',
    operation: 'insert',
    arguments: {
      'email': const FieldType.string(nullable: false),
      'name': const FieldType.string(),
    },
    invalidatesViews: ['v_user', 'v_user_summary'],
    invalidatesFactTables: ['tf_signup'],
  );

  schema.mutation(
    'placeOrder',
    returnType: 'Order',
    sqlSource: 'fn_place_order',
    operation: 'insert',
    inject: {'user_id': 'jwt:sub'},
    invalidatesViews: ['v_order_summary'],
    invalidatesFactTables: ['tf_sale'],
  );

  return schema;
}

void main() {
  final fixture = Platform.environment['FRAISEQL_CONFORMANCE_FIXTURE'];
  final out = Platform.environment['FRAISEQL_CONFORMANCE_OUT'];

  if (fixture == null || out == null) {
    stderr.writeln(
      'FRAISEQL_CONFORMANCE_FIXTURE and FRAISEQL_CONFORMANCE_OUT must be set',
    );
    exit(2);
  }

  final schema = switch (fixture) {
    'minimal' => authorMinimal(),
    'full' => authorFull(),
    _ => throw ArgumentError('unknown fixture $fixture'),
  };

  schema.exportJson(out);
}
