<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * One action of an observer (#2147): a webhook, a Slack message or an email, with the
 * keys the server's observer runtime reads. `fraiseql compile` checks them against it.
 */
final class ObserverAction
{
    /** The variable a Slack action reads its incoming-webhook URL from, unless told another. */
    public const DEFAULT_SLACK_WEBHOOK_ENV = 'SLACK_WEBHOOK_URL';

    /** @param array<string, mixed> $config */
    private function __construct(private readonly array $config)
    {
    }

    /**
     * An HTTP request to `$url`, or to the URL the server reads from `$urlEnv` at delivery.
     *
     * @param string|null $bodyTemplate The body, a template with `{{ field }}` placeholders.
     * @param string|null $signingSecretEnv The variable holding the key the request is signed with.
     * @param array<string, string> $headers Headers sent as written.
     * @param array<string, string> $headersEnv Headers whose values the server reads from these variables.
     *
     * @throws \InvalidArgumentException Neither or both of `$url` and `$urlEnv` are given.
     */
    public static function webhook(
        ?string $url = null,
        ?string $urlEnv = null,
        ?string $method = null,
        ?string $bodyTemplate = null,
        ?string $signingSecretEnv = null,
        array $headers = [],
        array $headersEnv = [],
    ): self {
        if (($url === null) === ($urlEnv === null)) {
            throw new \InvalidArgumentException(
                'ObserverAction::webhook(): give exactly one of $url and $urlEnv (the destination).'
            );
        }
        $config = ['type' => 'webhook'];
        $optional = [
            'url' => $url,
            'url_env' => $urlEnv,
            'method' => $method,
            'body_template' => $bodyTemplate,
            'signing_secret_env' => $signingSecretEnv,
            'headers' => $headers === [] ? null : $headers,
            'headers_env' => $headersEnv === [] ? null : $headersEnv,
        ];
        foreach ($optional as $key => $value) {
            if ($value !== null) {
                $config[$key] = $value;
            }
        }
        return new self($config);
    }

    /**
     * Post `$message`, a `{{ field }}` template, to `$channel`. The incoming-webhook URL
     * is read from {@see self::DEFAULT_SLACK_WEBHOOK_ENV} unless `$webhookUrl` (written
     * into the schema) or `$webhookUrlEnv` names another.
     *
     * @throws \InvalidArgumentException Both `$webhookUrl` and `$webhookUrlEnv` are given.
     */
    public static function slack(
        string $channel,
        string $message,
        ?string $webhookUrl = null,
        ?string $webhookUrlEnv = null,
    ): self {
        if ($webhookUrl !== null && $webhookUrlEnv !== null) {
            throw new \InvalidArgumentException(
                'ObserverAction::slack(): give at most one of $webhookUrl and $webhookUrlEnv (the destination).'
            );
        }
        $config = ['type' => 'slack', 'channel' => $channel, 'message_template' => $message];
        if ($webhookUrl !== null) {
            $config['webhook_url'] = $webhookUrl;
        } else {
            $config['webhook_url_env'] = $webhookUrlEnv ?? self::DEFAULT_SLACK_WEBHOOK_ENV;
        }
        return new self($config);
    }

    /**
     * An email to `$to`; `$body` is a `{{ field }}` template. The sender is the server's
     * SMTP configuration: an action cannot set one.
     */
    public static function email(string $to, string $subject, string $body): self
    {
        return new self(['type' => 'email', 'to' => $to, 'subject' => $subject, 'body_template' => $body]);
    }

    /** @return array<string, mixed> */
    public function toArray(): array
    {
        return $this->config;
    }
}
