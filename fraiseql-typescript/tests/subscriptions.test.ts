import { SchemaRegistry, registerSubscription } from "../src/index";

describe("Subscriptions", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  describe("registerSubscription with basic configuration", () => {
    it("should register a basic subscription", () => {
      registerSubscription(
        "userCreated",
        "User",
        false,
        [],
        "Subscribe to new users"
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions).toHaveLength(1);
      expect(schema.subscriptions[0].name).toBe("userCreated");
      expect(schema.subscriptions[0].entity_type).toBe("User");
    });

    it("should register subscription with topic", () => {
      registerSubscription(
        "orderCreated",
        "Order",
        false,
        [],
        "Subscribe to new orders",
        { topic: "order_events" }
      );

      const schema = SchemaRegistry.getSchema();
      const subscription = schema.subscriptions[0];

      expect(subscription.name).toBe("orderCreated");
      expect(subscription.topic).toBe("order_events");
    });

    it("should register subscription with operation filter", () => {
      registerSubscription(
        "userUpdated",
        "User",
        false,
        [],
        "Subscribe to user updates",
        { operation: "UPDATE" }
      );

      const schema = SchemaRegistry.getSchema();
      const subscription = schema.subscriptions[0];

      expect(subscription.operation).toBe("UPDATE");
    });

    it("should register subscription with multiple operation filters", () => {
      registerSubscription(
        "userChanged",
        "User",
        false,
        [],
        "Subscribe to user creates and updates",
        { operations: ["CREATE", "UPDATE"] }
      );

      const schema = SchemaRegistry.getSchema();
      const subscription = schema.subscriptions[0];

      expect(subscription.operations).toEqual(["CREATE", "UPDATE"]);
    });

    it("should register subscription with description", () => {
      const description = "Subscribe to any changes in orders";
      registerSubscription(
        "orderChanged",
        "Order",
        false,
        [],
        description,
        { operations: ["CREATE", "UPDATE", "DELETE"] }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].description).toBe(description);
    });
  });

  describe("registerSubscription with filter arguments", () => {
    it("should register subscription with single filter argument", () => {
      registerSubscription(
        "orderCreatedForUser",
        "Order",
        false,
        [{ name: "userId", type: "ID", nullable: false }],
        "Subscribe to orders for specific user"
      );

      const schema = SchemaRegistry.getSchema();
      const subscription = schema.subscriptions[0];

      expect(subscription.arguments).toHaveLength(1);
      expect(subscription.arguments[0].name).toBe("userId");
      expect(subscription.arguments[0].type).toBe("ID");
    });

    it("should register subscription with multiple filter arguments", () => {
      registerSubscription(
        "orderStatusChanged",
        "Order",
        false,
        [
          { name: "orderId", type: "ID", nullable: false },
          { name: "minAmount", type: "Decimal", nullable: true },
          { name: "maxAmount", type: "Decimal", nullable: true },
        ],
        "Subscribe to order status changes with filters"
      );

      const schema = SchemaRegistry.getSchema();
      const subscription = schema.subscriptions[0];

      expect(subscription.arguments).toHaveLength(3);
      expect(subscription.arguments[0].nullable).toBe(false);
      expect(subscription.arguments[1].nullable).toBe(true);
    });

    it("should register subscription with arguments and default values", () => {
      registerSubscription(
        "recentOrders",
        "Order",
        false,
        [
          { name: "limit", type: "Int", nullable: false, default: 10 },
          { name: "minAmount", type: "Decimal", nullable: true },
        ],
        "Subscribe to recent orders with optional filters"
      );

      const schema = SchemaRegistry.getSchema();
      const subscription = schema.subscriptions[0];

      expect(subscription.arguments[0].default).toBe(10);
      expect(subscription.arguments[1].default).toBeUndefined();
    });
  });

  describe("registerSubscription with nullable results", () => {
    it("should register nullable subscription", () => {
      registerSubscription("optionalUpdate", "Order", true, []);

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].nullable).toBe(true);
    });

    it("should register non-nullable subscription", () => {
      registerSubscription("requiredUpdate", "Order", false, []);

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].nullable).toBe(false);
    });
  });

  describe("registerSubscription with complex configurations", () => {
    it("should register subscription with all options combined", () => {
      registerSubscription(
        "orderLifecycle",
        "Order",
        false,
        [
          { name: "customerId", type: "ID", nullable: false },
          { name: "minAmount", type: "Decimal", nullable: true },
        ],
        "Subscribe to full order lifecycle events",
        {
          topic: "order_events",
          operations: ["CREATE", "UPDATE", "DELETE"],
        }
      );

      const schema = SchemaRegistry.getSchema();
      const subscription = schema.subscriptions[0];

      expect(subscription.name).toBe("orderLifecycle");
      expect(subscription.entity_type).toBe("Order");
      expect(subscription.topic).toBe("order_events");
      expect(subscription.operations).toEqual(["CREATE", "UPDATE", "DELETE"]);
      expect(subscription.arguments).toHaveLength(2);
      expect(subscription.description).toBe(
        "Subscribe to full order lifecycle events"
      );
    });

    it("should register multiple subscriptions for same entity with different filters", () => {
      registerSubscription(
        "orderCreated",
        "Order",
        false,
        [{ name: "customerId", type: "ID", nullable: false }],
        "New orders",
        { operation: "CREATE" }
      );

      registerSubscription(
        "orderUpdated",
        "Order",
        false,
        [{ name: "orderId", type: "ID", nullable: false }],
        "Order updates",
        { operation: "UPDATE" }
      );

      registerSubscription(
        "orderCancelled",
        "Order",
        false,
        [{ name: "orderId", type: "ID", nullable: false }],
        "Cancelled orders",
        { operation: "DELETE" }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions).toHaveLength(3);
      expect(schema.subscriptions[0].operation).toBe("CREATE");
      expect(schema.subscriptions[1].operation).toBe("UPDATE");
      expect(schema.subscriptions[2].operation).toBe("DELETE");
    });
  });

  describe("registerSubscription with different entity types", () => {
    it("should register subscriptions for multiple entity types", () => {
      registerSubscription("userCreated", "User", false, []);
      registerSubscription("postCreated", "Post", false, []);
      registerSubscription("commentCreated", "Comment", false, []);

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions).toHaveLength(3);
      expect(schema.subscriptions.map((s) => s.entity_type)).toEqual([
        "User",
        "Post",
        "Comment",
      ]);
    });
  });

  describe("registerSubscription with event filtering patterns", () => {
    it("should support INSERT event pattern", () => {
      registerSubscription(
        "onInsert",
        "Entity",
        false,
        [],
        "Subscribe to inserts",
        { operation: "CREATE" }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].operation).toBe("CREATE");
    });

    it("should support UPDATE event pattern", () => {
      registerSubscription(
        "onUpdate",
        "Entity",
        false,
        [],
        "Subscribe to updates",
        { operation: "UPDATE" }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].operation).toBe("UPDATE");
    });

    it("should support DELETE event pattern", () => {
      registerSubscription(
        "onDelete",
        "Entity",
        false,
        [],
        "Subscribe to deletes",
        { operation: "DELETE" }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].operation).toBe("DELETE");
    });

    it("should support multiple operations", () => {
      registerSubscription(
        "onChange",
        "Entity",
        false,
        [],
        "Subscribe to any changes",
        { operations: ["CREATE", "UPDATE", "DELETE"] }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].operations).toEqual([
        "CREATE",
        "UPDATE",
        "DELETE",
      ]);
    });
  });

  describe("registerSubscription with topic patterns", () => {
    it("should support topic-based subscriptions", () => {
      registerSubscription(
        "paymentProcessed",
        "Payment",
        false,
        [],
        "Listen to payment events",
        { topic: "payments" }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].topic).toBe("payments");
    });

    it("should support hierarchical topic names", () => {
      registerSubscription("orderTopic", "Order", false, [], undefined, {
        topic: "orders.events.lifecycle",
      });

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions[0].topic).toBe("orders.events.lifecycle");
    });

    it("should support topic with operation filtering", () => {
      registerSubscription(
        "newOrders",
        "Order",
        false,
        [],
        "New orders from topic",
        { topic: "orders", operation: "CREATE" }
      );

      const schema = SchemaRegistry.getSchema();
      const sub = schema.subscriptions[0];

      expect(sub.topic).toBe("orders");
      expect(sub.operation).toBe("CREATE");
    });
  });

  describe("Schema export with subscriptions", () => {
    it("should export schema with subscriptions", () => {
      registerSubscription("userCreated", "User", false, [], "New users");
      registerSubscription("orderUpdated", "Order", false, [], "Order changes", {
        operation: "UPDATE",
      });

      const schema = SchemaRegistry.getSchema();
      expect(schema.subscriptions).toBeDefined();
      expect(schema.subscriptions).toHaveLength(2);
    });

    it("should preserve subscription configuration in JSON export", () => {
      registerSubscription(
        "orderEvent",
        "Order",
        false,
        [
          { name: "customerId", type: "ID", nullable: false },
          { name: "minAmount", type: "Decimal", nullable: true },
        ],
        "Order lifecycle",
        {
          topic: "orders",
          operations: ["CREATE", "UPDATE"],
        }
      );

      const schema = SchemaRegistry.getSchema();
      const json = JSON.stringify(schema, null, 2);
      const parsed = JSON.parse(json);

      const sub = parsed.subscriptions[0];
      expect(sub.name).toBe("orderEvent");
      expect(sub.entity_type).toBe("Order");
      expect(sub.topic).toBe("orders");
      expect(sub.operations).toEqual(["CREATE", "UPDATE"]);
      expect(sub.arguments).toHaveLength(2);
    });
  });

  describe("Common subscription patterns", () => {
    it("should support CDC (Change Data Capture) pattern", () => {
      registerSubscription(
        "userChanges",
        "User",
        false,
        [],
        "Capture all user changes",
        { operations: ["CREATE", "UPDATE", "DELETE"] }
      );

      const schema = SchemaRegistry.getSchema();
      const sub = schema.subscriptions[0];

      expect(sub.operations).toEqual(["CREATE", "UPDATE", "DELETE"]);
    });

    it("should support filtering pattern", () => {
      registerSubscription(
        "expensiveOrders",
        "Order",
        false,
        [
          { name: "minAmount", type: "Decimal", nullable: false },
          { name: "currency", type: "String", nullable: true },
        ],
        "Orders above threshold",
        { operation: "CREATE" }
      );

      const schema = SchemaRegistry.getSchema();
      const sub = schema.subscriptions[0];

      expect(sub.arguments).toHaveLength(2);
      expect(sub.operation).toBe("CREATE");
    });

    it("should support real-time notification pattern", () => {
      registerSubscription(
        "newMessages",
        "Message",
        false,
        [{ name: "userId", type: "ID", nullable: false }],
        "Real-time messages for user",
        { topic: "messages", operation: "CREATE" }
      );

      const schema = SchemaRegistry.getSchema();
      const sub = schema.subscriptions[0];

      expect(sub.topic).toBe("messages");
      expect(sub.operation).toBe("CREATE");
      expect(sub.arguments[0].name).toBe("userId");
    });
  });
});
