import { SchemaRegistry, registerSubscription } from "../src/index";

/**
 * The members `IntermediateSubscription` accepts, and the only ones it accepts — it is
 * `deny_unknown_fields`, so anything else fails the *whole document* at
 * `fraiseql compile`, not just the subscription.
 *
 * Pinning the set here is what this suite was missing (#1024): every assertion below
 * used to check that the registry echoed back whatever it was handed, which it did
 * faithfully — including `entity_type`, `nullable`, `operation` and a free-form
 * `operations` that no SDK type ever declared. 25 green tests over a shape no schema
 * could compile.
 */
const COMPILER_MEMBERS = [
  "name",
  "return_type",
  "arguments",
  "description",
  "topic",
  "filter",
  "fields",
  "deprecated",
];

describe("Subscriptions", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  describe("the emitted shape is the compiler's", () => {
    it("emits return_type, and no key outside the compiler's member list", () => {
      registerSubscription(
        "orderLifecycle",
        "Order",
        [
          { name: "customerId", type: "ID", nullable: false },
          { name: "minAmount", type: "Decimal", nullable: true },
        ],
        "Subscribe to full order lifecycle events",
        {
          topic: "order_events",
          filter: { conditions: [{ argument: "customerId", path: "$.customer_id" }] },
          fields: ["id", "total"],
          deprecated: "use orderEvents",
        }
      );

      const subscription = SchemaRegistry.getSchema().subscriptions[0];

      expect(subscription.return_type).toBe("Order");
      expect(Object.keys(subscription).sort()).toEqual([...COMPILER_MEMBERS].sort());
    });

    it("omits every optional member the author did not set", () => {
      registerSubscription("userCreated", "User", []);

      const subscription = SchemaRegistry.getSchema().subscriptions[0];

      expect(Object.keys(subscription)).toEqual(["name", "return_type", "arguments"]);
    });
  });

  describe("registerSubscription with basic configuration", () => {
    it("should register a basic subscription", () => {
      registerSubscription("userCreated", "User", [], "Subscribe to new users");

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions).toHaveLength(1);
      expect(schema.subscriptions[0].name).toBe("userCreated");
      expect(schema.subscriptions[0].return_type).toBe("User");
    });

    it("should register subscription with topic", () => {
      registerSubscription("orderCreated", "Order", [], "Subscribe to new orders", {
        topic: "order_events",
      });

      const subscription = SchemaRegistry.getSchema().subscriptions[0];

      expect(subscription.name).toBe("orderCreated");
      expect(subscription.topic).toBe("order_events");
    });

    it("should register subscription with description", () => {
      const description = "Subscribe to any changes in orders";
      registerSubscription("orderChanged", "Order", [], description);

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].description).toBe(description);
    });

    it("should reject a duplicate subscription name", () => {
      registerSubscription("orderCreated", "Order", []);

      expect(() => registerSubscription("orderCreated", "Order", [])).toThrow(
        /already registered/
      );
    });
  });

  describe("registerSubscription with filter arguments", () => {
    it("should register subscription with single filter argument", () => {
      registerSubscription(
        "orderCreatedForUser",
        "Order",
        [{ name: "userId", type: "ID", nullable: false }],
        "Subscribe to orders for specific user"
      );

      const subscription = SchemaRegistry.getSchema().subscriptions[0];

      expect(subscription.arguments).toHaveLength(1);
      expect(subscription.arguments[0].name).toBe("userId");
      expect(subscription.arguments[0].type).toBe("ID");
    });

    it("should register subscription with multiple filter arguments", () => {
      registerSubscription(
        "orderStatusChanged",
        "Order",
        [
          { name: "orderId", type: "ID", nullable: false },
          { name: "minAmount", type: "Decimal", nullable: true },
          { name: "maxAmount", type: "Decimal", nullable: true },
        ],
        "Subscribe to order status changes with filters"
      );

      const subscription = SchemaRegistry.getSchema().subscriptions[0];

      expect(subscription.arguments).toHaveLength(3);
      expect(subscription.arguments[0].nullable).toBe(false);
      expect(subscription.arguments[1].nullable).toBe(true);
    });

    it("should register subscription with arguments and default values", () => {
      registerSubscription(
        "recentOrders",
        "Order",
        [
          { name: "limit", type: "Int", nullable: false, default: 10 },
          { name: "minAmount", type: "Decimal", nullable: true },
        ],
        "Subscribe to recent orders with optional filters"
      );

      const subscription = SchemaRegistry.getSchema().subscriptions[0];

      expect(subscription.arguments[0].default).toBe(10);
      expect(subscription.arguments[1].default).toBeUndefined();
    });
  });

  describe("registerSubscription with event matching", () => {
    // What replaced the CREATE/UPDATE/DELETE describe block. A subscription does not
    // filter on a DML verb — the runtime has no such member — it maps its own arguments
    // onto JSON paths in the event payload, and the compiler lowers those to
    // `filter.argument_paths`.
    it("should map one argument onto an event path", () => {
      registerSubscription(
        "orderUpdated",
        "Order",
        [{ name: "orderId", type: "ID", nullable: true }],
        "Updates for one order",
        { filter: { conditions: [{ argument: "orderId", path: "$.id" }] } }
      );

      const subscription = SchemaRegistry.getSchema().subscriptions[0];

      expect(subscription.filter).toEqual({
        conditions: [{ argument: "orderId", path: "$.id" }],
      });
    });

    it("should map several arguments onto several event paths", () => {
      registerSubscription(
        "orderNarrowed",
        "Order",
        [
          { name: "orderId", type: "ID", nullable: true },
          { name: "status", type: "String", nullable: true },
        ],
        undefined,
        {
          filter: {
            conditions: [
              { argument: "orderId", path: "$.id" },
              { argument: "status", path: "$.order_status" },
            ],
          },
        }
      );

      const subscription = SchemaRegistry.getSchema().subscriptions[0];

      expect(subscription.filter?.conditions).toHaveLength(2);
      expect(subscription.filter?.conditions[1]).toEqual({
        argument: "status",
        path: "$.order_status",
      });
    });

    it("should project a subset of fields from the event", () => {
      registerSubscription("orderTotals", "Order", [], undefined, {
        fields: ["id", "total"],
      });

      expect(SchemaRegistry.getSchema().subscriptions[0].fields).toEqual(["id", "total"]);
    });

    it("should drop an empty projection rather than emit one", () => {
      registerSubscription("orderAll", "Order", [], undefined, { fields: [] });

      expect(SchemaRegistry.getSchema().subscriptions[0].fields).toBeUndefined();
    });
  });

  describe("registerSubscription deprecation", () => {
    it("should carry a stated reason", () => {
      registerSubscription("legacyFeed", "Order", [], undefined, {
        deprecated: "use orderEvents",
      });

      expect(SchemaRegistry.getSchema().subscriptions[0].deprecated).toEqual({
        reason: "use orderEvents",
      });
    });

    it("should carry deprecation with no stated reason", () => {
      registerSubscription("legacyFeed", "Order", [], undefined, { deprecated: true });

      expect(SchemaRegistry.getSchema().subscriptions[0].deprecated).toEqual({});
    });

    it("should drop the key when not deprecated", () => {
      registerSubscription("liveFeed", "Order", [], undefined, { deprecated: false });

      expect(SchemaRegistry.getSchema().subscriptions[0].deprecated).toBeUndefined();
    });
  });

  describe("registerSubscription with different return types", () => {
    it("should register subscriptions for multiple return types", () => {
      registerSubscription("userCreated", "User", []);
      registerSubscription("postCreated", "Post", []);
      registerSubscription("commentCreated", "Comment", []);

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions).toHaveLength(3);
      expect(schema.subscriptions.map((s) => s.return_type)).toEqual([
        "User",
        "Post",
        "Comment",
      ]);
    });

    it("should register multiple subscriptions on the same type with different filters", () => {
      registerSubscription(
        "orderCreated",
        "Order",
        [{ name: "customerId", type: "ID", nullable: false }],
        "New orders",
        { filter: { conditions: [{ argument: "customerId", path: "$.customer_id" }] } }
      );

      registerSubscription(
        "orderUpdated",
        "Order",
        [{ name: "orderId", type: "ID", nullable: false }],
        "Order updates",
        { filter: { conditions: [{ argument: "orderId", path: "$.id" }] } }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions).toHaveLength(2);
      expect(schema.subscriptions.map((s) => s.filter?.conditions[0]?.path)).toEqual([
        "$.customer_id",
        "$.id",
      ]);
    });
  });

  describe("registerSubscription with topic patterns", () => {
    it("should support topic-based subscriptions", () => {
      registerSubscription("paymentProcessed", "Payment", [], "Listen to payment events", {
        topic: "payments",
      });

      expect(SchemaRegistry.getSchema().subscriptions[0].topic).toBe("payments");
    });

    it("should support hierarchical topic names", () => {
      registerSubscription("orderTopic", "Order", [], undefined, {
        topic: "orders.events.lifecycle",
      });

      expect(SchemaRegistry.getSchema().subscriptions[0].topic).toBe(
        "orders.events.lifecycle"
      );
    });

    it("should support topic with event-path filtering", () => {
      registerSubscription(
        "newOrders",
        "Order",
        [{ name: "status", type: "String", nullable: true }],
        "New orders from topic",
        {
          topic: "orders",
          filter: { conditions: [{ argument: "status", path: "$.order_status" }] },
        }
      );

      const sub = SchemaRegistry.getSchema().subscriptions[0];

      expect(sub.topic).toBe("orders");
      expect(sub.filter?.conditions[0]?.argument).toBe("status");
    });
  });

  describe("Schema export with subscriptions", () => {
    it("should export schema with subscriptions", () => {
      registerSubscription("userCreated", "User", [], "New users");
      registerSubscription("orderUpdated", "Order", [], "Order changes", {
        topic: "order_events",
      });

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions).toBeDefined();
      expect(schema.subscriptions).toHaveLength(2);
    });

    it("should survive a JSON round-trip with no key outside the compiler's set", () => {
      registerSubscription(
        "orderEvent",
        "Order",
        [
          { name: "customerId", type: "ID", nullable: false },
          { name: "minAmount", type: "Decimal", nullable: true },
        ],
        "Order lifecycle",
        {
          topic: "orders",
          filter: { conditions: [{ argument: "customerId", path: "$.customer_id" }] },
        }
      );

      const parsed = JSON.parse(JSON.stringify(SchemaRegistry.getSchema(), null, 2));
      const sub = parsed.subscriptions[0];

      expect(sub.name).toBe("orderEvent");
      expect(sub.return_type).toBe("Order");
      expect(sub.topic).toBe("orders");
      expect(sub.arguments).toHaveLength(2);
      expect(Object.keys(sub).every((k) => COMPILER_MEMBERS.includes(k))).toBe(true);
    });
  });

  describe("Common subscription patterns", () => {
    it("should support CDC (Change Data Capture) pattern", () => {
      registerSubscription("userChanges", "User", [], "Capture all user changes", {
        topic: "user_events",
      });

      const sub = SchemaRegistry.getSchema().subscriptions[0];

      expect(sub.topic).toBe("user_events");
      expect(sub.return_type).toBe("User");
    });

    it("should support filtering pattern", () => {
      registerSubscription(
        "expensiveOrders",
        "Order",
        [
          { name: "minAmount", type: "Decimal", nullable: false },
          { name: "currency", type: "String", nullable: true },
        ],
        "Orders above threshold",
        { filter: { conditions: [{ argument: "currency", path: "$.currency" }] } }
      );

      const sub = SchemaRegistry.getSchema().subscriptions[0];

      expect(sub.arguments).toHaveLength(2);
      expect(sub.filter?.conditions[0]?.path).toBe("$.currency");
    });

    it("should support real-time notification pattern", () => {
      registerSubscription(
        "newMessages",
        "Message",
        [{ name: "userId", type: "ID", nullable: false }],
        "Real-time messages for user",
        {
          topic: "messages",
          filter: { conditions: [{ argument: "userId", path: "$.recipient_id" }] },
        }
      );

      const sub = SchemaRegistry.getSchema().subscriptions[0];

      expect(sub.topic).toBe("messages");
      expect(sub.filter?.conditions[0]?.path).toBe("$.recipient_id");
      expect(sub.arguments[0].name).toBe("userId");
    });
  });
});
