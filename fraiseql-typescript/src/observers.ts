/**
 * Observer authoring API for FraiseQL.
 *
 * Observers react to database changes with configurable actions like webhooks,
 * Slack notifications, and emails.
 *
 * @example
 * ```typescript
 * import { Type, Observer, webhook, slack, email, RetryConfig } from "fraiseql";
 *
 * @Type()
 * class Order {
 *   id: number;
 *   status: string;
 *   total: number;
 * }
 *
 * @Observer({
 *   entity: "Order",
 *   event: "INSERT",
 *   condition: "status == 'paid'",
 *   actions: [webhook("https://api.example.com/orders")]
 * })
 * function onOrderCreated() {
 *   // Triggered when a paid order is created
 * }
 * ```
 *
 * @packageDocumentation
 */

import { SchemaRegistry } from "./registry";

/**
 * Retry configuration for observer actions.
 */
export interface RetryConfig {
  /** Maximum number of retry attempts (default: 3) */
  max_attempts: number;
  /** Backoff strategy: "exponential", "linear", or "fixed" (default: "exponential") */
  backoff_strategy: "exponential" | "linear" | "fixed";
  /** Initial delay in milliseconds (default: 100) */
  initial_delay_ms: number;
  /** Maximum delay in milliseconds (default: 60000) */
  max_delay_ms: number;
}

/**
 * Default retry configuration.
 */
export const DEFAULT_RETRY_CONFIG: RetryConfig = {
  max_attempts: 3,
  backoff_strategy: "exponential",
  initial_delay_ms: 100,
  max_delay_ms: 60000,
};

/**
 * Webhook action configuration.
 */
export interface WebhookAction {
  type: "webhook";
  url?: string;
  url_env?: string;
  headers: Record<string, string>;
  body_template?: string;
  [key: string]: unknown;
}

/**
 * Slack action configuration.
 */
export interface SlackAction {
  type: "slack";
  channel: string;
  message: string;
  webhook_url?: string;
  webhook_url_env?: string;
  [key: string]: unknown;
}

/**
 * Email action configuration.
 */
export interface EmailAction {
  type: "email";
  to: string;
  subject: string;
  body: string;
  from?: string;
  [key: string]: unknown;
}

/**
 * Union of all action types.
 */
export type Action = WebhookAction | SlackAction | EmailAction;

/**
 * Observer definition.
 */
export interface ObserverDefinition {
  name: string;
  entity: string;
  event: "INSERT" | "UPDATE" | "DELETE";
  actions: Action[];
  condition?: string;
  retry: RetryConfig;
}

/**
 * Observer decorator configuration.
 */
export interface ObserverConfig {
  /** Entity type to observe (e.g., "Order") */
  entity: string;
  /** Event type: INSERT, UPDATE, or DELETE */
  event: "INSERT" | "UPDATE" | "DELETE";
  /** List of actions to execute */
  actions: Action[];
  /** Optional condition expression (e.g., "status == 'paid'") */
  condition?: string;
  /** Optional retry configuration */
  retry?: RetryConfig;
}

/**
 * Observer decorator.
 *
 * Defines an observer that reacts to database changes (INSERT, UPDATE, DELETE)
 * with configurable actions like webhooks, Slack notifications, and emails.
 *
 * @param config - Observer configuration
 * @returns Decorator function
 *
 * @example
 * ```typescript
 * @Observer({
 *   entity: "Order",
 *   event: "INSERT",
 *   condition: "total > 1000",
 *   actions: [
 *     webhook("https://api.example.com/orders"),
 *     slack("#orders", "New order {id}: ${total}"),
 *     email("admin@example.com", "Order created", "Order {id}")
 *   ]
 * })
 * function onHighValueOrder() {
 *   // Triggered when high-value order is created
 * }
 * ```
 */
export function Observer(config: ObserverConfig) {
  return function (target: any, propertyKey?: string) {
    const name = propertyKey || target.name;

    const observerDef: ObserverDefinition = {
      name,
      entity: config.entity,
      event: config.event.toUpperCase() as "INSERT" | "UPDATE" | "DELETE",
      actions: config.actions,
      condition: config.condition,
      retry: config.retry || DEFAULT_RETRY_CONFIG,
    };

    // Register with schema registry
    SchemaRegistry.registerObserver(
      observerDef.name,
      observerDef.entity,
      observerDef.event,
      observerDef.actions,
      observerDef.condition,
      observerDef.retry
    );

    return target;
  };
}

/**
 * Create a webhook action.
 *
 * @param url - Static webhook URL (or undefined if using url_env)
 * @param options - Webhook options
 * @returns Webhook action configuration
 *
 * @example
 * ```typescript
 * // Static URL
 * webhook("https://api.example.com/orders")
 *
 * // Environment variable
 * webhook({ url_env: "ORDER_WEBHOOK_URL" })
 *
 * // With custom headers
 * webhook("https://api.example.com/orders", {
 *   headers: { "Authorization": "Bearer token123" }
 * })
 *
 * // With body template
 * webhook("https://api.example.com/orders", {
 *   body_template: '{"order_id": "{{id}}", "total": {{total}}}'
 * })
 * ```
 */
export function webhook(
  url?: string,
  options?: {
    url_env?: string;
    headers?: Record<string, string>;
    body_template?: string;
  }
): WebhookAction {
  if (!url && !options?.url_env) {
    throw new Error("Either url or url_env must be provided");
  }

  const action: WebhookAction = {
    type: "webhook",
    headers: options?.headers || { "Content-Type": "application/json" },
  };

  if (url) {
    action.url = url;
  }
  if (options?.url_env) {
    action.url_env = options.url_env;
  }
  if (options?.body_template) {
    action.body_template = options.body_template;
  }

  return action;
}

/**
 * Create a Slack notification action.
 *
 * @param channel - Slack channel (e.g., "#orders")
 * @param message - Message template (supports {field} placeholders)
 * @param options - Slack options
 * @returns Slack action configuration
 *
 * @example
 * ```typescript
 * // Basic usage
 * slack("#orders", "New order {id}: ${total}")
 *
 * // Custom webhook URL
 * slack("#orders", "New order {id}", {
 *   webhook_url: "https://hooks.slack.com/services/..."
 * })
 *
 * // Custom environment variable
 * slack("#alerts", "Alert!", {
 *   webhook_url_env: "SLACK_ALERTS_WEBHOOK"
 * })
 * ```
 */
export function slack(
  channel: string,
  message: string,
  options?: {
    webhook_url?: string;
    webhook_url_env?: string;
  }
): SlackAction {
  const action: SlackAction = {
    type: "slack",
    channel,
    message,
  };

  if (options?.webhook_url) {
    action.webhook_url = options.webhook_url;
  }
  if (options?.webhook_url_env) {
    action.webhook_url_env = options.webhook_url_env;
  } else if (!options?.webhook_url) {
    // Default to SLACK_WEBHOOK_URL
    action.webhook_url_env = "SLACK_WEBHOOK_URL";
  }

  return action;
}

/**
 * Create an email action.
 *
 * @param to - Recipient email address
 * @param subject - Email subject (supports {field} placeholders)
 * @param body - Email body (supports {field} placeholders)
 * @param options - Email options
 * @returns Email action configuration
 *
 * @example
 * ```typescript
 * // Basic usage
 * email(
 *   "admin@example.com",
 *   "Order {id} created",
 *   "Order {id} for ${total} was created"
 * )
 *
 * // With sender
 * email(
 *   "customer@example.com",
 *   "Your order {id} has shipped",
 *   "Your order is on its way!",
 *   { from_email: "noreply@example.com" }
 * )
 * ```
 */
export function email(
  to: string,
  subject: string,
  body: string,
  options?: {
    from_email?: string;
  }
): EmailAction {
  const action: EmailAction = {
    type: "email",
    to,
    subject,
    body,
  };

  if (options?.from_email) {
    action.from = options.from_email;
  }

  return action;
}
