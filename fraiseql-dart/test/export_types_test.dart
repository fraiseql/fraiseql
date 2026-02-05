import 'package:test/test.dart';
import 'package:fraiseql/schema.dart';
import 'dart:convert';
import 'dart:io';

void main() {
  setUp(() => Schema.reset());
  tearDown(() => Schema.reset());

  test('export types minimal single type', () {
    Schema.registerType('User', {
      'id': {'type': 'ID', 'nullable': false},
      'name': {'type': 'String', 'nullable': false},
      'email': {'type': 'String', 'nullable': false},
    }, description: 'User in the system');

    final json = Schema.exportTypes(pretty: true);
    final parsed = jsonDecode(json);

    expect(parsed, containsPair('types', isNotEmpty));
    expect(parsed['types'], hasLength(1));
    expect(parsed, isNot(containsKey('queries')));
    expect(parsed, isNot(containsKey('mutations')));

    final userDef = parsed['types'][0];
    expect(userDef['name'], equals('User'));
    expect(userDef['description'], equals('User in the system'));
  });

  test('export types multiple types', () {
    Schema.registerType('User', {
      'id': {'type': 'ID', 'nullable': false},
      'name': {'type': 'String', 'nullable': false},
    });
    Schema.registerType('Post', {
      'id': {'type': 'ID', 'nullable': false},
      'title': {'type': 'String', 'nullable': false},
    });

    final json = Schema.exportTypes(pretty: true);
    final parsed = jsonDecode(json);

    expect(parsed['types'], hasLength(2));
    final typeNames = parsed['types'].map((t) => t['name']).toList();
    expect(typeNames, containsAll(['User', 'Post']));
  });

  test('export types no queries', () {
    Schema.registerType('User', {
      'id': {'type': 'ID', 'nullable': false},
    });

    final json = Schema.exportTypes(pretty: true);
    final parsed = jsonDecode(json);

    expect(parsed, containsPair('types', isNotEmpty));
    expect(parsed, isNot(containsKey('queries')));
  });

  test('export types compact format', () {
    Schema.registerType('User', {
      'id': {'type': 'ID', 'nullable': false},
    });

    final compact = Schema.exportTypes(false);
    final pretty = Schema.exportTypes(true);

    expect(compact.length, lessThanOrEqualTo(pretty.length));
    expect(jsonDecode(compact), containsPair('types', anything));
  });

  test('export types pretty format', () {
    Schema.registerType('User', {
      'id': {'type': 'ID', 'nullable': false},
    });

    final json = Schema.exportTypes(true);
    expect(json, contains('\n'));
    expect(jsonDecode(json), containsPair('types', anything));
  });

  test('export types file', () async {
    Schema.registerType('User', {
      'id': {'type': 'ID', 'nullable': false},
      'name': {'type': 'String', 'nullable': false},
    });

    final tmpFile = '/tmp/fraiseql_types_test_dart.json';
    if (File(tmpFile).existsSync()) File(tmpFile).deleteSync();

    Schema.exportTypesFile(tmpFile);

    expect(File(tmpFile).existsSync(), isTrue);
    final content = File(tmpFile).readAsStringSync();
    final parsed = jsonDecode(content);
    expect(parsed['types'], hasLength(1));

    File(tmpFile).deleteSync();
  });

  test('export types empty', () {
    final json = Schema.exportTypes(true);
    final parsed = jsonDecode(json);

    expect(parsed, containsPair('types', isEmpty));
  });
}
