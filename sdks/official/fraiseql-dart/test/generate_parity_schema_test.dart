// Generate parity schema for cross-SDK comparison.
//
// Produces the canonical parity-schema JSON compatible with the Python
// reference generator and compare_schemas.py.
//
// Usage:
//   dart test test/generate_parity_schema_test.dart
//
// When SCHEMA_OUTPUT_FILE is set the JSON is written to that path instead
// of stdout:
//   SCHEMA_OUTPUT_FILE=/tmp/schema_dart.json dart test test/generate_parity_schema_test.dart

import 'dart:convert';
import 'dart:io';
import 'package:test/test.dart';

void main() {
  test('GenerateParitySchema', () {
    // ── Types ──────────────────────────────────────────────────────────

    final types = [
      {
        'name': 'User',
        'sql_source': 'v_user',
        'fields': [
          _field('id', 'ID', false),
          _field('email', 'String', false),
          _field('name', 'String', false),
        ],
      },
      {
        'name': 'Order',
        'sql_source': 'v_order',
        'fields': [
          _field('id', 'ID', false),
          _field('total', 'Float', false),
        ],
      },
      {
        'name': 'UserNotFound',
        'sql_source': 'v_user_not_found',
        'is_error': true,
        'fields': [
          _field('message', 'String', false),
          _field('code', 'String', false),
        ],
      },
    ];

    // ── Queries ────────────────────────────────────────────────────────

    final queries = [
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
    ];

    // ── Mutations ──────────────────────────────────────────────────────

    final mutations = [
      {
        'name': 'createUser',
        'return_type': 'User',
        'sql_source': 'fn_create_user',
        'operation': 'insert',
        'arguments': [
          _argument('email', 'String', false),
          _argument('name', 'String', false),
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
    ];

    // ── Output ─────────────────────────────────────────────────────────

    final schema = {
      'types': types,
      'queries': queries,
      'mutations': mutations,
    };

    final encoder = JsonEncoder.withIndent('  ');
    final json = encoder.convert(schema);

    final outputFile = Platform.environment['SCHEMA_OUTPUT_FILE'];
    if (outputFile != null && outputFile.isNotEmpty) {
      File(outputFile).writeAsStringSync('$json\n');
    } else {
      stdout.writeln(json);
    }
  });
}

Map<String, dynamic> _field(String name, String type, bool nullable) =>
    {'name': name, 'type': type, 'nullable': nullable};

Map<String, dynamic> _argument(String name, String type, bool nullable) =>
    {'name': name, 'type': type, 'nullable': nullable};
