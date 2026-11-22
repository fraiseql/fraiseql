/// How an observer retries an action that fails (#2147). An observer given none runs
/// on the server's own policy.
final class RetryConfig {
  /// The attempts, the first included.
  final int maxAttempts;

  /// `'fixed'`, `'linear'` or `'exponential'`.
  final String backoffStrategy;

  /// The delay before the first retry.
  final int initialDelayMs;

  /// The longest delay between retries.
  final int maxDelayMs;

  const RetryConfig({
    required this.maxAttempts,
    required this.backoffStrategy,
    required this.initialDelayMs,
    required this.maxDelayMs,
  });

  /// The retry block, as the compiler reads it.
  Map<String, Object?> toJson() => {
        'max_attempts': maxAttempts,
        'backoff_strategy': backoffStrategy,
        'initial_delay_ms': initialDelayMs,
        'max_delay_ms': maxDelayMs,
      };
}

/// An action an observer runs (#2147): a webhook, a Slack message or an email, with
/// the keys the server's observer runtime reads. `fraiseql compile` checks them
/// against it.
final class ObserverAction {
  /// The variable a Slack action reads its incoming-webhook URL from, unless told
  /// another.
  static const defaultSlackWebhookEnv = 'SLACK_WEBHOOK_URL';

  final Map<String, Object?> _json;

  const ObserverAction._(this._json);

  /// An HTTP request to [url], or to the URL the server reads from [urlEnv] at
  /// delivery; exactly one of the two. [bodyTemplate] is a `{{ field }}` template,
  /// [signingSecretEnv] the variable holding the key the request is signed with, and
  /// [headersEnv] maps a header to the variable holding its value.
  ///
  /// Throws [ArgumentError] when neither or both of [url] and [urlEnv] are given.
  factory ObserverAction.webhook({
    String? url,
    String? urlEnv,
    String? method,
    String? bodyTemplate,
    String? signingSecretEnv,
    Map<String, String> headers = const {},
    Map<String, String> headersEnv = const {},
  }) {
    if ((url == null) == (urlEnv == null)) {
      throw ArgumentError(
          'ObserverAction.webhook: give exactly one of url and urlEnv (the destination).');
    }
    return ObserverAction._({
      'type': 'webhook',
      if (url != null) 'url': url,
      if (urlEnv != null) 'url_env': urlEnv,
      if (method != null) 'method': method,
      if (bodyTemplate != null) 'body_template': bodyTemplate,
      if (signingSecretEnv != null) 'signing_secret_env': signingSecretEnv,
      if (headers.isNotEmpty) 'headers': headers,
      if (headersEnv.isNotEmpty) 'headers_env': headersEnv,
    });
  }

  /// Posts [message], a `{{ field }}` template, to [channel], at the URL in
  /// [defaultSlackWebhookEnv] unless [webhookUrl] (written into the schema) or
  /// [webhookUrlEnv] names another.
  ///
  /// Throws [ArgumentError] when both [webhookUrl] and [webhookUrlEnv] are given.
  factory ObserverAction.slack(
    String channel,
    String message, {
    String? webhookUrl,
    String? webhookUrlEnv,
  }) {
    if (webhookUrl != null && webhookUrlEnv != null) {
      throw ArgumentError(
          'ObserverAction.slack: give at most one of webhookUrl and '
          'webhookUrlEnv (the destination).');
    }
    return ObserverAction._({
      'type': 'slack',
      'channel': channel,
      'message_template': message,
      if (webhookUrl != null)
        'webhook_url': webhookUrl
      else
        'webhook_url_env': webhookUrlEnv ?? defaultSlackWebhookEnv,
    });
  }

  /// An email to [to]; [body] is a `{{ field }}` template. The sender is the server's
  /// SMTP configuration: an action cannot set one.
  factory ObserverAction.email(String to, String subject, String body) =>
      ObserverAction._({
        'type': 'email',
        'to': to,
        'subject': subject,
        'body_template': body,
      });

  /// The action, as the compiler reads it.
  Map<String, Object?> toJson() => _json;
}
