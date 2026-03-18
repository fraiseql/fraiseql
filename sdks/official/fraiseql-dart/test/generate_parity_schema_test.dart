/// Generate parity schema for cross-SDK comparison.
///
/// Usage:
///   SCHEMA_OUTPUT_FILE=/tmp/schema_dart.json dart test test/generate_parity_schema_test.dart
import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

void main() {
  test('generate parity schema', () {
    final schema = {
      'types': [
        {
          'name': 'User',
          'sql_source': 'v_user',
          'fields': [
            {'name': 'id', 'type': 'ID', 'nullable': false},
            {'name': 'email', 'type': 'String', 'nullable': false},
            {'name': 'name', 'type': 'String', 'nullable': false},
          ],
        },
        {
          'name': 'Order',
          'sql_source': 'v_order',
          'fields': [
            {'name': 'id', 'type': 'ID', 'nullable': false},
            {'name': 'total', 'type': 'Float', 'nullable': false},
          ],
        },
        {
          'name': 'UserNotFound',
          'sql_source': 'v_user_not_found',
          'is_error': true,
          'fields': [
            {'name': 'message', 'type': 'String', 'nullable': false},
            {'name': 'code', 'type': 'String', 'nullable': false},
          ],
        },
      ],
      'queries': [
        {
          'name': 'users',
          'return_type': 'User',
          'returns_list': true,
          'nullable': false,
          'sql_source': 'v_user',
          'arguments': <Map<String, dynamic>>[],
        },
        {
          'name': 'tenantOrders',
          'return_type': 'Order',
          'returns_list': true,
          'nullable': false,
          'sql_source': 'v_order',
          'inject_params': {'tenant_id': 'jwt:tenant_id'},
          'cache_ttl_seconds': 300,
          'requires_role': 'admin',
          'arguments': <Map<String, dynamic>>[],
        },
      ],
      'mutations': [
        {
          'name': 'createUser',
          'return_type': 'User',
          'sql_source': 'fn_create_user',
          'operation': 'insert',
          'arguments': [
            {'name': 'email', 'type': 'String', 'nullable': false},
            {'name': 'name', 'type': 'String', 'nullable': false},
          ],
        },
        {
          'name': 'placeOrder',
          'return_type': 'Order',
          'sql_source': 'fn_place_order',
          'operation': 'insert',
          'inject_params': {'user_id': 'jwt:sub'},
          'invalidates_views': ['v_order_summary'],
          'invalidates_fact_tables': ['tf_sales'],
          'arguments': <Map<String, dynamic>>[],
        },
      ],
    };

    final json = const JsonEncoder.withIndent('  ').convert(schema);
    final outputFile = Platform.environment['SCHEMA_OUTPUT_FILE'];

    if (outputFile != null && outputFile.isNotEmpty) {
      File(outputFile).writeAsStringSync('$json\n');
    } else {
      // ignore: avoid_print
      print(json);
    }
  });
}
