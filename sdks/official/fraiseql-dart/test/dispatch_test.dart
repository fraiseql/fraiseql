import 'package:test/test.dart';
import 'package:fraiseql/fraiseql.dart';

void main() {
  group('SqlSourceDispatch annotation', () {
    test('creates dispatch config with explicit mapping', () {
      final dispatch = SqlSourceDispatch(
        argument: 'timeInterval',
        mapping: {
          'DAY': 'tf_orders_day',
          'WEEK': 'tf_orders_week',
          'MONTH': 'tf_orders_month',
        },
      );

      expect(dispatch.argument, equals('timeInterval'));
      expect(dispatch.mapping, isNotNull);
      expect(dispatch.mapping!['DAY'], equals('tf_orders_day'));
      expect(dispatch.template, isNull);
    });

    test('creates dispatch config with template', () {
      final dispatch = SqlSourceDispatch(
        argument: 'timeInterval',
        template: 'tf_orders_{value}',
      );

      expect(dispatch.argument, equals('timeInterval'));
      expect(dispatch.template, equals('tf_orders_{value}'));
      expect(dispatch.mapping, isNull);
    });

    test('validates mapping keys are strings', () {
      final dispatch = SqlSourceDispatch(
        argument: 'status',
        mapping: {
          'ACTIVE': 'v_active_users',
          'INACTIVE': 'v_inactive_users',
        },
      );

      expect(dispatch.mapping!.length, equals(2));
      expect(dispatch.mapping!.keys.every((_) => true), isTrue);
    });

    test('validates template is a string template', () {
      final dispatch = SqlSourceDispatch(
        argument: 'region',
        template: 'v_orders_{region_code}',
      );

      expect(dispatch.template!.contains('{'), isTrue);
      expect(dispatch.template!.endsWith('}'), isTrue);
    });

    test('allows null mapping and template separately', () {
      final dispatchWithMapping = SqlSourceDispatch(
        argument: 'arg1',
        mapping: {'A': 'source_a'},
      );
      expect(dispatchWithMapping.mapping, isNotNull);
      expect(dispatchWithMapping.template, isNull);

      final dispatchWithTemplate = SqlSourceDispatch(
        argument: 'arg2',
        template: 'source_{arg}',
      );
      expect(dispatchWithTemplate.template, isNotNull);
      expect(dispatchWithTemplate.mapping, isNull);
    });
  });
}
