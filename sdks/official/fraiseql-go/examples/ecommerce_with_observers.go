package main

import (
	"fmt"
	"log"

	"github.com/fraiseql/fraiseql-go/fraiseql"
)

// Order represents an e-commerce order
type Order struct {
	ID            string  `json:"id"`
	CustomerEmail string  `json:"customer_email"`
	Status        string  `json:"status"`
	Total         float64 `json:"total"`
	CreatedAt     string  `json:"created_at"`
}

// Payment represents a payment record
type Payment struct {
	ID          string   `json:"id"`
	OrderID     string   `json:"order_id"`
	Amount      float64  `json:"amount"`
	Status      string   `json:"status"`
	ProcessedAt *string  `json:"processed_at"`
}

func main() {
	// Register types
	fraiseql.RegisterType("Order", []fraiseql.FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "customer_email", Type: "String", Nullable: false},
		{Name: "status", Type: "String", Nullable: false},
		{Name: "total", Type: "Float", Nullable: false},
		{Name: "created_at", Type: "DateTime", Nullable: false},
	}, "E-commerce order")

	fraiseql.RegisterType("Payment", []fraiseql.FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "order_id", Type: "ID", Nullable: false},
		{Name: "amount", Type: "Float", Nullable: false},
		{Name: "status", Type: "String", Nullable: false},
		{Name: "processed_at", Type: "DateTime", Nullable: true},
	}, "Payment record")

	// Observer 1: Notify when high-value orders are created
	fraiseql.NewObserver("onHighValueOrder").
		Entity("Order").
		Event("INSERT").
		Condition("total > 1000").
		Actions(
			fraiseql.Webhook("https://api.example.com/high-value-orders"),
			fraiseql.Slack("#sales", "🎉 High-value order {id}: ${total}"),
			fraiseql.EmailAction(
				"sales@example.com",
				"High-value order {id}",
				"Order {id} for ${total} was created by {customer_email}",
			),
		).
		Register()

	// Observer 2: Notify when orders are shipped
	fraiseql.NewObserver("onOrderShipped").
		Entity("Order").
		Event("UPDATE").
		Condition("status.changed() and status == 'shipped'").
		Actions(
			fraiseql.WebhookWithEnv("SHIPPING_WEBHOOK_URL"),
			fraiseql.EmailAction(
				"{customer_email}",
				"Your order {id} has shipped!",
				"Your order is on its way. Track it here: https://example.com/track/{id}",
				map[string]interface{}{
					"from_email": "noreply@example.com",
				},
			),
		).
		Register()

	// Observer 3: Alert on payment failures with aggressive retry
	fraiseql.NewObserver("onPaymentFailure").
		Entity("Payment").
		Event("UPDATE").
		Condition("status == 'failed'").
		Actions(
			fraiseql.Slack("#payments", "⚠️ Payment failed for order {order_id}: {amount}"),
			fraiseql.Webhook("https://api.example.com/payment-failures", map[string]interface{}{
				"headers": map[string]string{
					"Authorization": "Bearer {PAYMENT_API_TOKEN}",
				},
			}),
		).
		Retry(fraiseql.RetryConfig{
			MaxAttempts:     5,
			BackoffStrategy: "exponential",
			InitialDelayMs:  100,
			MaxDelayMs:      60000,
		}).
		Register()

	// Observer 4: Archive deleted orders
	fraiseql.NewObserver("onOrderDeleted").
		Entity("Order").
		Event("DELETE").
		Action(fraiseql.Webhook("https://api.example.com/archive", map[string]interface{}{
			"body_template": `{"type": "order", "id": "{{id}}", "data": {{_json}}}`,
		})).
		Register()

	// Observer 5: Simple notification for all new orders
	fraiseql.NewObserver("onOrderCreated").
		Entity("Order").
		Event("INSERT").
		Action(fraiseql.Slack("#orders", "New order {id} by {customer_email}")).
		Register()

	// Export schema
	err := fraiseql.ExportSchema("ecommerce_schema.json")
	if err != nil {
		log.Fatalf("Failed to export schema: %v", err)
	}

	fmt.Println("\n🎯 Observer Summary:")
	fmt.Println("   1. onHighValueOrder → Webhooks, Slack, Email for total > 1000")
	fmt.Println("   2. onOrderShipped → Webhook + customer email when status='shipped'")
	fmt.Println("   3. onPaymentFailure → Slack + webhook with retry on payment failures")
	fmt.Println("   4. onOrderDeleted → Archive deleted orders via webhook")
	fmt.Println("   5. onOrderCreated → Slack notification for all new orders")
	fmt.Println("\n✨ Next steps:")
	fmt.Println("   1. fraiseql-cli compile ecommerce_schema.json")
	fmt.Println("   2. fraiseql-server --schema ecommerce_schema.compiled.json")
	fmt.Println("   3. Observers will execute automatically on database changes!")
}
