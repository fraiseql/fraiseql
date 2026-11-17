/**
 * Observer authoring API for FraiseQL event-driven workflows.
 *
 * Observers watch for database changes and trigger actions (webhooks, Slack, email).
 * NO runtime behavior - only used for schema compilation.
 *
 * Every action factory emits exactly the keys the server's observer runtime reads.
 * `fraiseql compile` refuses any other key, because the runtime would ignore it and the
 * action would deliver nothing.
 *
 * @example
 * ```typescript
 * import { Observer, webhook, slack } from "fraiseql";
 *
 * class Observers {
 *   @Observer({ entity: "Order", event: "INSERT", actions: [webhook("https://...")] })
 *   onOrderCreated() {}
 * }
 * ```
 */

import { decoratedMemberName } from "./decorator-name";
import { SchemaRegistry, ObserverAction, ObserverRetryConfig } from "./registry";

/**
 * Retry configuration for observer actions.
 */
export type RetryConfig = ObserverRetryConfig;

/**
 * Default retry configuration used when no retry is specified.
 */
export const DEFAULT_RETRY_CONFIG: RetryConfig = {
  max_attempts: 3,
  backoff_strategy: "exponential",
  initial_delay_ms: 100,
  max_delay_ms: 60000,
};

/**
 * Configuration for the @Observer decorator.
 */
interface ObserverConfig {
  entity: string;
  event: string;
  actions: ObserverAction[];
  condition?: string;
  retry?: RetryConfig;
}

/**
 * Method decorator to register an observer with the schema registry.
 *
 * @param config - Observer configuration
 * @returns Method decorator
 */
export function Observer(config: ObserverConfig) {
  return function (
    // eslint-disable-next-line @typescript-eslint/no-explicit-any -- method decorator target
    _target: any,
    propertyKeyOrContext: unknown,
    _descriptor?: PropertyDescriptor
  ): void {
    const name = decoratedMemberName(propertyKeyOrContext, "Observer");
    SchemaRegistry.registerObserver(
      name,
      config.entity,
      config.event,
      config.actions,
      config.condition,
      config.retry
    );
  };
}

/**
 * Options for the webhook action factory.
 */
interface WebhookOptions {
  /** Environment variable holding the URL, read by the server at delivery. */
  url_env?: string;
  /** HTTP method; the server sends `POST` when unset. */
  method?: string;
  headers?: Record<string, string>;
  /** Request body with `{{ field }}` placeholders naming fields of the observed entity. */
  body_template?: string;
  /**
   * Environment variable holding the HMAC key the server signs each request with.
   * The key itself is never authored: a compiled schema is a build artifact.
   */
  signing_secret_env?: string;
}

/**
 * Create a webhook action.
 *
 * @param url - Static URL (or use `url_env`)
 * @param options - Additional options: url_env, method, headers, body_template,
 *   signing_secret_env
 * @returns Webhook action definition
 * @throws If neither url nor url_env is provided
 */
export function webhook(url?: string, options?: WebhookOptions): ObserverAction {
  if (url === undefined && options?.url_env === undefined) {
    throw new Error("Either url or url_env must be provided");
  }
  const action: ObserverAction & {
    headers: Record<string, string>;
    url?: string;
    url_env?: string;
    method?: string;
    body_template?: string;
    signing_secret_env?: string;
  } = {
    type: "webhook",
    headers: { "Content-Type": "application/json", ...(options?.headers ?? {}) },
  };
  if (url !== undefined) {
    action.url = url;
  }
  if (options?.url_env !== undefined) {
    action.url_env = options.url_env;
  }
  if (options?.method !== undefined) {
    action.method = options.method;
  }
  if (options?.body_template !== undefined) {
    action.body_template = options.body_template;
  }
  if (options?.signing_secret_env !== undefined) {
    action.signing_secret_env = options.signing_secret_env;
  }
  return action;
}

/**
 * Options for the slack action factory.
 */
interface SlackOptions {
  webhook_url?: string;
  webhook_url_env?: string;
}

/**
 * Create a Slack notification action.
 *
 * @param channel - Slack channel (e.g. "#orders")
 * @param message - Message template with `{{ field }}` placeholders, emitted as
 *   `message_template`
 * @param options - Optional webhook URL or environment variable override
 * @returns Slack action definition
 */
export function slack(channel: string, message: string, options?: SlackOptions): ObserverAction {
  const action: ObserverAction & {
    channel: string;
    message_template: string;
    webhook_url_env: string;
    webhook_url?: string;
  } = {
    type: "slack",
    channel,
    message_template: message,
    webhook_url_env: options?.webhook_url_env ?? "SLACK_WEBHOOK_URL",
  };
  if (options?.webhook_url !== undefined) {
    action.webhook_url = options.webhook_url;
  }
  return action;
}

/**
 * Create an email notification action.
 *
 * The sender is the server's SMTP configuration; an action cannot set one.
 *
 * @param to - Recipient email address
 * @param subject - Email subject
 * @param body - Email body template with `{{ field }}` placeholders, emitted as
 *   `body_template`
 * @returns Email action definition
 */
export function email(to: string, subject: string, body: string): ObserverAction {
  return {
    type: "email",
    to,
    subject,
    body_template: body,
  };
}
