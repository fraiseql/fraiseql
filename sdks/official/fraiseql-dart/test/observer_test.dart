import 'dart:convert';

import 'package:fraiseql/fraiseql.dart';
import 'package:test/test.dart';

/// Observer authoring (#2147): an observer per action kind, as the compiler reads it.
void main() {
  List<Map<String, Object?>> observers(FraiseQLSchema schema) =>
      ((jsonDecode(jsonEncode(schema.toJson()))
              as Map<String, Object?>)['observers']! as List<Object?>)
          .cast<Map<String, Object?>>();

  Map<String, Object?> actionOf(ObserverAction action) {
    final schema = FraiseQLSchema()
      ..observer('probe', entity: 'Order', event: 'INSERT', actions: [action]);
    return (observers(schema).single['actions']! as List<Object?>).single!
        as Map<String, Object?>;
  }

  test('a webhook observer carries every key it is given', () {
    final schema = FraiseQLSchema()
      ..observer(
        'orderPlacedWebhook',
        entity: 'Order',
        event: 'INSERT',
        actions: [
          ObserverAction.webhook(
            urlEnv: 'HOOK_URL',
            method: 'PUT',
            bodyTemplate: '{"total": {{ total }}}',
            signingSecretEnv: 'HOOK_KEY',
            headers: {'X-Source': 'tests'},
            headersEnv: {'Authorization': 'HOOK_TOKEN'},
          ),
        ],
        retry: const RetryConfig(
          maxAttempts: 5,
          backoffStrategy: 'linear',
          initialDelayMs: 200,
          maxDelayMs: 5000,
        ),
      );

    expect(observers(schema), [
      {
        'name': 'orderPlacedWebhook',
        'entity': 'Order',
        'event': 'INSERT',
        'actions': [
          {
            'type': 'webhook',
            'url_env': 'HOOK_URL',
            'method': 'PUT',
            'body_template': '{"total": {{ total }}}',
            'signing_secret_env': 'HOOK_KEY',
            'headers': {'X-Source': 'tests'},
            'headers_env': {'Authorization': 'HOOK_TOKEN'},
          },
        ],
        'retry': {
          'max_attempts': 5,
          'backoff_strategy': 'linear',
          'initial_delay_ms': 200,
          'max_delay_ms': 5000,
        },
      },
    ]);
  });

  test('a webhook given only its url authors only its url', () {
    expect(actionOf(ObserverAction.webhook(url: 'https://hooks.example/x')),
        {'type': 'webhook', 'url': 'https://hooks.example/x'});
  });

  test('a webhook has exactly one destination', () {
    final refusal = throwsA(isA<ArgumentError>().having((e) => e.message,
        'message', contains('exactly one of url and urlEnv')));
    expect(() => ObserverAction.webhook(), refusal);
    expect(
        () => ObserverAction.webhook(
            url: 'https://hooks.example/x', urlEnv: 'HOOK_URL'),
        refusal);
  });

  test('a slack action reads the default env unless told otherwise', () {
    expect(
        actionOf(ObserverAction.slack('#orders', 'Order {{ id }} shipped')), {
      'type': 'slack',
      'channel': '#orders',
      'message_template': 'Order {{ id }} shipped',
      'webhook_url_env': 'SLACK_WEBHOOK_URL',
    });
    expect(
        actionOf(ObserverAction.slack('#o', 'm', webhookUrlEnv: 'OPS_SLACK')), {
      'type': 'slack',
      'channel': '#o',
      'message_template': 'm',
      'webhook_url_env': 'OPS_SLACK',
    });
    // A written URL is the one destination: the default env is not emitted beside it.
    expect(
        actionOf(ObserverAction.slack('#o', 'm',
            webhookUrl: 'https://hooks.slack.example/x')),
        {
          'type': 'slack',
          'channel': '#o',
          'message_template': 'm',
          'webhook_url': 'https://hooks.slack.example/x',
        });
  });

  test('a slack action has one destination', () {
    expect(
        () => ObserverAction.slack('#o', 'm',
            webhookUrl: 'https://x', webhookUrlEnv: 'OPS_SLACK'),
        throwsA(isA<ArgumentError>().having((e) => e.message, 'message',
            contains('at most one of webhookUrl and webhookUrlEnv'))));
  });

  test('an email action names its body template', () {
    expect(
        actionOf(ObserverAction.email(
            'ops@example.com', 'Order deleted', 'Order {{ id }}')),
        {
          'type': 'email',
          'to': 'ops@example.com',
          'subject': 'Order deleted',
          'body_template': 'Order {{ id }}',
        });
  });

  test('a condition is emitted and an unset retry is left out', () {
    final schema = FraiseQLSchema()
      ..observer('orderShipped',
          entity: 'Order',
          event: 'UPDATE',
          condition: "status == 'shipped'",
          actions: [ObserverAction.email('ops@example.com', 's', 'b')]);
    final definition = observers(schema).single;
    expect(definition['condition'], "status == 'shipped'");
    // No retry: the server's own policy applies, so no block is written.
    expect(definition.containsKey('retry'), isFalse);
  });

  test('no observer leaves the section out', () {
    expect(FraiseQLSchema().toJson().containsKey('observers'), isFalse);
  });

  test('an observer name is declared once', () {
    final schema = FraiseQLSchema();
    void declare() => schema.observer('orderCreated',
        entity: 'Order',
        event: 'INSERT',
        actions: [ObserverAction.email('a@example.com', 's', 'b')]);
    declare();
    expect(
        declare,
        throwsA(isA<ArgumentError>().having((e) => e.message, 'message',
            contains("observer 'orderCreated' is already declared"))));
  });
}
