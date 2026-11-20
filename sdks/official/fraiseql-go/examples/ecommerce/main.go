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
	ID          string  `json:"id"`
	OrderID     string  `json:"order_id"`
	Amount      float64 `json:"amount"`
	Status      string  `json:"status"`
	ProcessedAt *string `json:"processed_at"`
}

// must stops the example on a registration the SDK refuses.
func must(err error) {
	if err != nil {
		log.Fatal(err)
	}
}

// Observers react to database changes with webhooks, Slack messages and emails.
//
// Templates use `{{ field }}` placeholders that name fields of the observed entity by
// their stored key. The compiler refuses a webhook body_template naming a field the
// entity does not hold.
func main() {
	// Register types
	must(fraiseql.RegisterType("Order", []fraiseql.FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "customer_email", Type: "String", Nullable: false},
		{Name: "status", Type: "String", Nullable: false},
		{Name: "total", Type: "Float", Nullable: false},
		{Name: "created_at", Type: "DateTime", Nullable: false},
	}, "E-commerce order"))

	must(fraiseql.RegisterType("Payment", []fraiseql.FieldInfo{
		{Name: "id", Type: "ID", Nullable: false},
		{Name: "order_id", Type: "ID", Nullable: false},
		{Name: "amount", Type: "Float", Nullable: false},
		{Name: "status", Type: "String", Nullable: false},
		{Name: "processed_at", Type: "DateTime", Nullable: true},
	}, "Payment record"))

	// Observer 1: Notify when high-value orders are created
	must(fraiseql.NewObserver("onHighValueOrder").
		Entity("Order").
		Event("INSERT").
		Condition("total > 1000").
		Actions(
			fraiseql.Webhook("https://api.example.com/high-value-orders"),
			fraiseql.Slack("#sales", "🎉 High-value order {{ id }}: ${{ total }}"),
			fraiseql.EmailAction(
				"sales@example.com",
				"High-value order",
				"Order {{ id }} for ${{ total }} was created by {{ customer_email }}",
			),
		).
		Register())

	// Observer 2: Notify when orders are shipped
	must(fraiseql.NewObserver("onOrderShipped").
		Entity("Order").
		Event("UPDATE").
		Condition("field_changed_to('status', 'shipped')").
		Actions(
			fraiseql.WebhookWithEnv("SHIPPING_WEBHOOK_URL",
				fraiseql.WebhookSigningSecretEnv("SHIPPING_WEBHOOK_SECRET"),
			),
			fraiseql.EmailAction(
				"fulfilment@example.com",
				"An order has shipped",
				"Order {{ id }} for {{ customer_email }} is on its way: https://example.com/track/{{ id }}",
			),
		).
		Register())

	// Observer 3: Alert on payment failures with aggressive retry
	must(fraiseql.NewObserver("onPaymentFailure").
		Entity("Payment").
		Event("UPDATE").
		Condition("status == 'failed'").
		Actions(
			fraiseql.Slack("#payments", "⚠️ Payment failed for order {{ order_id }}: {{ amount }}"),
			fraiseql.Webhook("https://api.example.com/payment-failures",
				fraiseql.WebhookHeader("X-Source", "fraiseql"),
				fraiseql.WebhookSigningSecretEnv("PAYMENT_WEBHOOK_SECRET"),
			),
		).
		Retry(fraiseql.RetryConfig{
			MaxAttempts:     5,
			BackoffStrategy: "exponential",
			InitialDelayMs:  100,
			MaxDelayMs:      60000,
		}).
		Register())

	// Observer 4: Archive deleted orders
	must(fraiseql.NewObserver("onOrderDeleted").
		Entity("Order").
		Event("DELETE").
		Action(fraiseql.Webhook("https://api.example.com/archive",
			fraiseql.WebhookBodyTemplate(`{"type": "order", "id": "{{ id }}", "status": "{{ status }}"}`),
		)).
		Register())

	// Observer 5: Simple notification for all new orders
	must(fraiseql.NewObserver("onOrderCreated").
		Entity("Order").
		Event("INSERT").
		Action(fraiseql.Slack("#orders", "New order {{ id }} by {{ customer_email }}")).
		Register())

	must(fraiseql.ExportSchema("schema.json"))

	fmt.Println("\n🎯 Observers defined:")
	fmt.Println("   1. onHighValueOrder → Webhooks, Slack, Email for total > 1000")
	fmt.Println("   2. onOrderShipped → Webhook + email when status becomes 'shipped'")
	fmt.Println("   3. onPaymentFailure → Slack + webhook with retry on payment failures")
	fmt.Println("   4. onOrderDeleted → Archive deleted orders via webhook")
	fmt.Println("   5. onOrderCreated → Slack notification for all new orders")
	fmt.Println("\n✨ Next steps:")
	fmt.Println("   1. Enable the observer runtime in fraiseql.toml:")
	fmt.Println("        [observers]")
	fmt.Println("        enabled = true")
	fmt.Println("   2. fraiseql-cli compile schema.json")
	fmt.Println("      The compiler refuses the declarations while [observers] is not enabled.")
}
