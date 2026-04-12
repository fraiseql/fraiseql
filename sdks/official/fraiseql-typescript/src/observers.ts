/**
 * Observer authoring API for FraiseQL event-driven workflows.
 *
 * Observers watch for database changes and trigger actions (webhooks, Slack, email).
 * NO runtime behavior - only used for schema compilation.
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
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy method decorator target
  return function (_target: any, propertyKey: string, _descriptor: PropertyDescriptor): void {
    SchemaRegistry.registerObserver(
      propertyKey,
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
  url_env?: string;
  headers?: Record<string, string>;
  body_template?: string;
}

/**
 * Create a webhook action.
 *
 * @param url - Static URL (mutually exclusive with url_env)
 * @param options - Additional options including url_env, headers, body_template
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
    body_template?: string;
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
  if (options?.body_template !== undefined) {
    action.body_template = options.body_template;
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
 * @param message - Message template with {field} placeholders
 * @param options - Optional webhook URL or environment variable override
 * @returns Slack action definition
 */
export function slack(channel: string, message: string, options?: SlackOptions): ObserverAction {
  const action: ObserverAction & {
    channel: string;
    message: string;
    webhook_url_env: string;
    webhook_url?: string;
  } = {
    type: "slack",
    channel,
    message,
    webhook_url_env: options?.webhook_url_env ?? "SLACK_WEBHOOK_URL",
  };
  if (options?.webhook_url !== undefined) {
    action.webhook_url = options.webhook_url;
  }
  return action;
}

/**
 * Options for the email action factory.
 */
interface EmailOptions {
  from_email?: string;
}

/**
 * Create an email notification action.
 *
 * @param to - Recipient email address
 * @param subject - Email subject template with {field} placeholders
 * @param body - Email body template with {field} placeholders
 * @param options - Optional sender override
 * @returns Email action definition
 */
export function email(
  to: string,
  subject: string,
  body: string,
  options?: EmailOptions
): ObserverAction {
  const action: ObserverAction & {
    to: string;
    subject: string;
    body: string;
    from?: string;
  } = {
    type: "email",
    to,
    subject,
    body,
  };
  if (options?.from_email !== undefined) {
    action.from = options.from_email;
  }
  return action;
}
