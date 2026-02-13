import 'dart:convert';
import 'package:test/test.dart';
import 'package:fraiseql_dart/schema.dart';

void main() {
  setUp(() => Schema.reset());
  tearDown(() => Schema.reset());

  // MARK: - Field Creation Tests (3 tests)

  test('field should create with all properties', () {
    final fields = {
      'email': {
        'type': 'String',
        'nullable': false,
        'description': 'User email address',
        'requires_scope': 'read:user.email',
      }
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    expect(typeInfo, isNotNull);
    expect(typeInfo!.fields['email']?['type'], equals('String'));
    expect(typeInfo.fields['email']?['nullable'], equals(false));
    expect(typeInfo.fields['email']?['description'],
        equals('User email address'));
    expect(typeInfo.fields['email']?['requires_scope'],
        equals('read:user.email'));
  });

  test('field should create with minimal properties', () {
    final fields = {
      'id': {'type': 'Int'}
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    expect(typeInfo, isNotNull);
    expect(typeInfo!.fields['id']?['type'], equals('Int'));
    expect(typeInfo.fields['id']?['requires_scope'], isNull);
    expect(typeInfo.fields['id']?['requires_scopes'], isNull);
  });

  test('field should preserve metadata alongside scopes', () {
    final fields = {
      'password': {
        'type': 'String',
        'nullable': false,
        'description': 'Hashed password',
        'requires_scope': 'admin:user.*',
      }
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    expect(typeInfo!.fields['password']?['type'], equals('String'));
    expect(typeInfo.fields['password']?['requires_scope'],
        equals('admin:user.*'));
    expect(typeInfo.fields['password']?['description'],
        equals('Hashed password'));
  });

  // MARK: - Single Scope Requirement Tests (3 tests)

  test('field should support single scope format', () {
    final fields = {
      'email': {
        'type': 'String',
        'requires_scope': 'read:user.email',
      }
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    expect(typeInfo!.fields['email']?['requires_scope'],
        equals('read:user.email'));
    expect(typeInfo.fields['email']?['requires_scopes'], isNull);
  });

  test('field should support wildcard resource scope', () {
    final fields = {
      'profile': {
        'type': 'Object',
        'requires_scope': 'read:User.*',
      }
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    expect(typeInfo!.fields['profile']?['requires_scope'],
        equals('read:User.*'));
  });

  test('field should support global wildcard scope', () {
    final fields = {
      'secret': {
        'type': 'String',
        'requires_scope': 'admin:*',
      }
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    expect(typeInfo!.fields['secret']?['requires_scope'], equals('admin:*'));
  });

  // MARK: - Multiple Scopes Array Tests (3 tests)

  test('field should support multiple scopes array', () {
    final fields = {
      'email': {
        'type': 'String',
        'requires_scopes': ['read:user.email', 'write:user.email'],
      }
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    final scopes = typeInfo!.fields['email']?['requires_scopes'] as List?;
    expect(scopes, isNotNull);
    expect(scopes?.length, equals(2));
    expect(scopes, contains('read:user.email'));
    expect(scopes, contains('write:user.email'));
  });

  test('field should support single element scopes array', () {
    final fields = {
      'profile': {
        'type': 'Object',
        'requires_scopes': ['read:user.profile'],
      }
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    final scopes = typeInfo!.fields['profile']?['requires_scopes'] as List?;
    expect(scopes, isNotNull);
    expect(scopes?.length, equals(1));
    expect(scopes?[0], equals('read:user.profile'));
  });

  test('field should support complex scopes array', () {
    final fields = {
      'data': {
        'type': 'String',
        'requires_scopes': [
          'read:user.email',
          'write:user.*',
          'admin:*',
        ],
      }
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    final scopes = typeInfo!.fields['data']?['requires_scopes'] as List?;
    expect(scopes, isNotNull);
    expect(scopes?.length, equals(3));
  });

  // MARK: - Scope Pattern Validation Tests (6 tests)

  test('scope validator should validate specific field scope', () {
    final fields = {
      'email': {
        'type': 'String',
        'requires_scope': 'read:user.email',
      }
    };

    expect(() => Schema.registerType('User', fields), returnsNormally);
  });

  test('scope validator should validate resource wildcard scope', () {
    final fields = {
      'profile': {
        'type': 'Object',
        'requires_scope': 'read:User.*',
      }
    };

    expect(() => Schema.registerType('User', fields), returnsNormally);
  });

  test('scope validator should validate global admin wildcard', () {
    final fields = {
      'secret': {
        'type': 'String',
        'requires_scope': 'admin:*',
      }
    };

    expect(() => Schema.registerType('User', fields), returnsNormally);
  });

  test('scope validator should reject scope missing colon', () {
    final fields = {
      'data': {
        'type': 'String',
        'requires_scope': 'readuser',
      }
    };

    expect(
      () => Schema.registerType('User', fields),
      throwsFormatException,
    );
  });

  test('scope validator should reject action with hyphen', () {
    final fields = {
      'data': {
        'type': 'String',
        'requires_scope': 'read-all:user',
      }
    };

    expect(
      () => Schema.registerType('User', fields),
      throwsFormatException,
    );
  });

  test('scope validator should reject resource with hyphen', () {
    final fields = {
      'data': {
        'type': 'String',
        'requires_scope': 'read:user-data',
      }
    };

    expect(
      () => Schema.registerType('User', fields),
      throwsFormatException,
    );
  });

  // MARK: - Schema Registry Tests (3 tests)

  test('schema should register type with fields and scopes', () {
    final fields = {
      'id': {'type': 'Int', 'nullable': false},
      'email': {
        'type': 'String',
        'nullable': false,
        'requires_scope': 'read:user.email',
      }
    };

    Schema.registerType('User', fields);

    final typeNames = Schema.getTypeNames();
    expect(typeNames, contains('User'));
  });

  test('schema should extract scoped fields from registry', () {
    final fields = {
      'id': {'type': 'Int', 'nullable': false},
      'email': {
        'type': 'String',
        'nullable': false,
        'requires_scope': 'read:user.email',
      },
      'password': {
        'type': 'String',
        'nullable': false,
        'requires_scope': 'admin:user.password',
      }
    };

    Schema.registerType('User', fields);

    final typeInfo = Schema.getType('User');
    expect(typeInfo, isNotNull);
    expect(
      typeInfo!.fields['email']?['requires_scope'],
      equals('read:user.email'),
    );
    expect(
      typeInfo.fields['password']?['requires_scope'],
      equals('admin:user.password'),
    );
  });

  test('schema should handle multiple types with different scopes', () {
    Schema.registerType('User', {
      'id': {'type': 'Int'},
      'email': {
        'type': 'String',
        'requires_scope': 'read:user.email',
      }
    });

    Schema.registerType('Post', {
      'id': {'type': 'Int'},
      'content': {
        'type': 'String',
        'requires_scope': 'read:post.content',
      }
    });

    final typeNames = Schema.getTypeNames();
    expect(typeNames.length, equals(2));
    expect(typeNames, contains('User'));
    expect(typeNames, contains('Post'));
  });

  // MARK: - JSON Export Tests (2 tests)

  test('schema export should include scope in field JSON', () {
    final fields = {
      'email': {
        'type': 'String',
        'nullable': false,
        'requires_scope': 'read:user.email',
      }
    };

    Schema.registerType('User', fields);
    final json = Schema.exportTypes(pretty: false);

    expect(json, contains('User'));
    expect(json, contains('email'));
    expect(json, contains('read:user.email'));
    expect(json, contains('requires_scope'));
  });

  test('schema export should export multiple types with scopes', () {
    Schema.registerType('User', {
      'id': {'type': 'Int'},
      'email': {
        'type': 'String',
        'requires_scope': 'read:user.email',
      }
    });

    Schema.registerType('Post', {
      'id': {'type': 'Int'},
      'content': {
        'type': 'String',
        'requires_scope': 'read:post.content',
      }
    });

    final json = Schema.exportTypes(pretty: false);

    expect(json, contains('User'));
    expect(json, contains('Post'));
    expect(json, contains('read:user.email'));
    expect(json, contains('read:post.content'));
  });

  // MARK: - Conflicting Scope/Scopes Tests (2 tests)

  test('field with both scope and scopes should be rejected', () {
    final fields = {
      'email': {
        'type': 'String',
        'requires_scope': 'read:user.email',
        'requires_scopes': ['write:user.email'],
      }
    };

    expect(
      () => Schema.registerType('User', fields),
      throwsFormatException,
    );
  });

  test('scope validator should reject empty scope string', () {
    final fields = {
      'data': {
        'type': 'String',
        'requires_scope': '',
      }
    };

    expect(
      () => Schema.registerType('User', fields),
      throwsFormatException,
    );
  });
}
