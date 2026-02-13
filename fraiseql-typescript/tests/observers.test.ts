/**
 * Tests for observer authoring API.
 */

import { describe, it, expect, beforeEach } from "@jest/globals";
import {
  Observer,
  webhook,
  slack,
  email,
  DEFAULT_RETRY_CONFIG,
  SchemaRegistry,
} from "../src/index";
import type { RetryConfig } from "../src/index";

beforeEach(() => {
  SchemaRegistry.clear();
});

describe("Observer decorator", () => {
  it("should register observer with schema registry", () => {
    class Observers {
      @Observer({
        entity: "Order",
        event: "INSERT",
        actions: [webhook("https://example.com")],
      })
      onOrderCreated() {}
    }
    void Observers;

    const schema = SchemaRegistry.getSchema();

    expect(schema.observers).toBeDefined();
    expect(schema.observers?.length).toBe(1);

    const observer = schema.observers![0];
    expect(observer.name).toBe("onOrderCreated");
    expect(observer.entity).toBe("Order");
    expect(observer.event).toBe("INSERT");
    expect(observer.actions.length).toBe(1);
    expect(observer.actions[0].type).toBe("webhook");
  });

  it("should support condition expressions", () => {
    class Observers {
      @Observer({
        entity: "Order",
        event: "UPDATE",
        condition: "status == 'paid'",
        actions: [webhook("https://example.com")],
      })
      onOrderPaid() {}
    }
    void Observers;

    const schema = SchemaRegistry.getSchema();
    const observer = schema.observers![0];

    expect(observer.condition).toBe("status == 'paid'");
  });

  it("should support custom retry configuration", () => {
    const retryConfig: RetryConfig = {
      max_attempts: 5,
      backoff_strategy: "linear",
      initial_delay_ms: 200,
      max_delay_ms: 30000,
    };

    class Observers {
      @Observer({
        entity: "Order",
        event: "INSERT",
        actions: [webhook("https://example.com")],
        retry: retryConfig,
      })
      onOrder() {}
    }
    void Observers;

    const schema = SchemaRegistry.getSchema();
    const observer = schema.observers![0];

    expect(observer.retry.max_attempts).toBe(5);
    expect(observer.retry.backoff_strategy).toBe("linear");
    expect(observer.retry.initial_delay_ms).toBe(200);
    expect(observer.retry.max_delay_ms).toBe(30000);
  });

  it("should use default retry config when not provided", () => {
    class Observers {
      @Observer({
        entity: "Order",
        event: "INSERT",
        actions: [webhook("https://example.com")],
      })
      onOrder() {}
    }
    void Observers;

    const schema = SchemaRegistry.getSchema();
    const observer = schema.observers![0];

    expect(observer.retry.max_attempts).toBe(DEFAULT_RETRY_CONFIG.max_attempts);
    expect(observer.retry.backoff_strategy).toBe(DEFAULT_RETRY_CONFIG.backoff_strategy);
  });

  it("should normalize event type to uppercase", () => {
    class Observers {
      @Observer({
        entity: "Order",
        event: "insert" as any, // Bypass type check for test
        actions: [webhook("https://example.com")],
      })
      onOrder() {}
    }
    void Observers;

    const schema = SchemaRegistry.getSchema();
    const observer = schema.observers![0];

    expect(observer.event).toBe("INSERT");
  });

  it("should support multiple actions", () => {
    class Observers {
      @Observer({
        entity: "Order",
        event: "INSERT",
        actions: [
          webhook("https://example.com/orders"),
          slack("#orders", "New order {id}"),
          email("admin@example.com", "Order created", "Order {id} created"),
        ],
      })
      onOrder() {}
    }
    void Observers;

    const schema = SchemaRegistry.getSchema();
    const observer = schema.observers![0];

    expect(observer.actions.length).toBe(3);
    expect(observer.actions[0].type).toBe("webhook");
    expect(observer.actions[1].type).toBe("slack");
    expect(observer.actions[2].type).toBe("email");
  });
});

describe("webhook action", () => {
  it("should create webhook with static URL", () => {
    const action = webhook("https://example.com/orders");

    expect(action.type).toBe("webhook");
    expect(action.url).toBe("https://example.com/orders");
    expect(action.headers).toEqual({ "Content-Type": "application/json" });
  });

  it("should create webhook with environment variable", () => {
    const action = webhook(undefined, { url_env: "ORDER_WEBHOOK_URL" });

    expect(action.type).toBe("webhook");
    expect(action.url_env).toBe("ORDER_WEBHOOK_URL");
    expect(action.url).toBeUndefined();
  });

  it("should support custom headers", () => {
    const action = webhook("https://example.com", {
      headers: { Authorization: "Bearer token123" },
    });

    expect(action.headers.Authorization).toBe("Bearer token123");
  });

  it("should support body template", () => {
    const action = webhook("https://example.com", {
      body_template: '{"order_id": "{{id}}"}',
    });

    expect(action.body_template).toBe('{"order_id": "{{id}}"}');
  });

  it("should throw error without URL or url_env", () => {
    expect(() => webhook()).toThrow("Either url or url_env must be provided");
  });
});

describe("slack action", () => {
  it("should create slack action with default webhook_url_env", () => {
    const action = slack("#orders", "New order {id}: ${total}");

    expect(action.type).toBe("slack");
    expect(action.channel).toBe("#orders");
    expect(action.message).toBe("New order {id}: ${total}");
    expect(action.webhook_url_env).toBe("SLACK_WEBHOOK_URL");
  });

  it("should support custom webhook URL", () => {
    const action = slack("#orders", "New order", {
      webhook_url: "https://hooks.slack.com/services/XXX",
    });

    expect(action.webhook_url).toBe("https://hooks.slack.com/services/XXX");
  });

  it("should support custom environment variable", () => {
    const action = slack("#alerts", "Alert!", {
      webhook_url_env: "SLACK_ALERTS_WEBHOOK",
    });

    expect(action.webhook_url_env).toBe("SLACK_ALERTS_WEBHOOK");
  });
});

describe("email action", () => {
  it("should create email action", () => {
    const action = email(
      "admin@example.com",
      "Order {id} created",
      "Order {id} for ${total} was created"
    );

    expect(action.type).toBe("email");
    expect(action.to).toBe("admin@example.com");
    expect(action.subject).toBe("Order {id} created");
    expect(action.body).toBe("Order {id} for ${total} was created");
  });

  it("should support custom sender", () => {
    const action = email(
      "customer@example.com",
      "Order shipped",
      "Your order is on its way!",
      { from_email: "noreply@example.com" }
    );

    expect(action.from).toBe("noreply@example.com");
  });
});

describe("Schema export", () => {
  it("should include observers in schema", () => {
    class Observers {
      @Observer({
        entity: "Order",
        event: "INSERT",
        actions: [webhook("https://example.com")],
      })
      onOrder1() {}

      @Observer({
        entity: "Order",
        event: "UPDATE",
        actions: [slack("#orders", "Updated")],
      })
      onOrder2() {}
    }
    void Observers;

    const schema = SchemaRegistry.getSchema();

    expect(schema.observers).toBeDefined();
    expect(schema.observers?.length).toBe(2);
    expect(schema.observers![0].name).toBe("onOrder1");
    expect(schema.observers![1].name).toBe("onOrder2");
  });

  it("should not include observers key when none defined", () => {
    const schema = SchemaRegistry.getSchema();

    expect(schema.observers).toBeUndefined();
  });
});
