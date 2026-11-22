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
  /**
   * Headers sent as written. Only these are authored: the server sends the payload as
   * JSON and sets its content type itself.
   */
  headers?: Record<string, string>;
  /** Request body with `{{ field }}` placeholders naming fields of the observed entity. */
  body_template?: string;
  /**
   * Environment variable holding the HMAC key the server signs each request with.
   * The key itself is never authored: a compiled schema is a build artifact.
   */
  signing_secret_env?: string;
  /**
   * Headers whose value the server reads from the environment at delivery: header name
   * to environment variable name. Use it for a credential (`Authorization`, an API key):
   * the compiler refuses a credential header written under `headers`, since a compiled
   * schema is a build artifact.
   */
  headers_env?: Record<string, string>;
}

/**
 * Create a webhook action.
 *
 * @param url - Static URL (or use `url_env`)
 * @param options - Additional options: url_env, method, headers, body_template,
 *   signing_secret_env, headers_env
 * @returns Webhook action definition
 * @throws If neither or both of url and url_env are provided: an action has one destination
 */
export function webhook(url?: string, options?: WebhookOptions): ObserverAction {
  if (url === undefined && options?.url_env === undefined) {
    throw new Error("Either url or url_env must be provided");
  }
  if (url !== undefined && options?.url_env !== undefined) {
    throw new Error("webhook: set one of url / url_env, not both");
  }
  const action: ObserverAction & {
    headers?: Record<string, string>;
    url?: string;
    url_env?: string;
    method?: string;
    body_template?: string;
    signing_secret_env?: string;
    headers_env?: Record<string, string>;
  } = {
    type: "webhook",
  };
  // Only the headers the author wrote: the server sends the payload as JSON and sets its
  // content type itself.
  if (options?.headers !== undefined) {
    action.headers = { ...options.headers };
  }
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
  if (options?.headers_env !== undefined) {
    action.headers_env = { ...options.headers_env };
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
 * @param options - Optional webhook URL or environment variable override. With neither,
 *   the URL is read from `SLACK_WEBHOOK_URL`; a written `webhook_url` is the one
 *   destination, so no default environment variable is emitted beside it.
 * @returns Slack action definition
 * @throws If both webhook_url and webhook_url_env are provided
 */
export function slack(channel: string, message: string, options?: SlackOptions): ObserverAction {
  if (options?.webhook_url !== undefined && options.webhook_url_env !== undefined) {
    throw new Error("slack: set one of webhook_url / webhook_url_env, not both");
  }
  const destination: { webhook_url: string } | { webhook_url_env: string } =
    options?.webhook_url !== undefined
      ? { webhook_url: options.webhook_url }
      : { webhook_url_env: options?.webhook_url_env ?? "SLACK_WEBHOOK_URL" };
  return {
    type: "slack",
    channel,
    message_template: message,
    ...destination,
  };
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
