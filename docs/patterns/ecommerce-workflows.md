<!-- Skip to main content -->
---
title: E-Commerce Platform with Complex Workflows
description: Complete guide to building a production e-commerce platform with order management, inventory tracking, and fulfillment workflows.
keywords: ["workflow", "saas", "realtime", "ecommerce", "analytics", "federation"]
tags: ["documentation", "reference"]
---

# E-Commerce Platform with Complex Workflows

**Status:** ✅ Production Ready
**Complexity:** ⭐⭐⭐⭐ (Advanced)
**Audience:** E-commerce architects, backend engineers
**Reading Time:** 25-30 minutes
**Last Updated:** 2026-02-05

Complete guide to building a production e-commerce platform with order management, inventory tracking, and fulfillment workflows.

---

## Schema Design

### Products & Inventory

```sql
<!-- Code example in SQL -->
CREATE TABLE products (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  sku VARCHAR(50) UNIQUE NOT NULL,
  name VARCHAR(255) NOT NULL,
  description TEXT,
  category_id UUID NOT NULL,
  brand VARCHAR(100),
  price DECIMAL(12, 2) NOT NULL,
  cost DECIMAL(12, 2),  -- For margin calculation
  status VARCHAR(50) NOT NULL,  -- active, draft, discontinued
  created_at TIMESTAMP DEFAULT NOW()
);

-- Product variants (sizes, colors, etc.)
CREATE TABLE product_variants (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  sku VARCHAR(50) UNIQUE NOT NULL,
  name VARCHAR(255),  -- e.g., "Red - Size M"
  price_modifier DECIMAL(10, 2),  -- +$5 for premium variant
  weight DECIMAL(8, 3),
  dimensions JSONB,  -- { width: 10, height: 20, depth: 5 }
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_product_id (product_id)
);

-- Inventory tracking (stock levels)
CREATE TABLE inventory (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  variant_id UUID NOT NULL UNIQUE REFERENCES product_variants(id),
  warehouse_id UUID NOT NULL,
  quantity_on_hand INT NOT NULL DEFAULT 0,
  quantity_reserved INT NOT NULL DEFAULT 0,
  quantity_available INT GENERATED ALWAYS AS (quantity_on_hand - quantity_reserved) STORED,
  reorder_point INT,
  reorder_quantity INT,
  last_stock_check TIMESTAMP,

  INDEX idx_warehouse_id (warehouse_id),
  INDEX idx_quantity_available (quantity_available)
);

-- Stock movements (audit trail)
CREATE TABLE stock_movements (
  id BIGSERIAL PRIMARY KEY,
  variant_id UUID NOT NULL REFERENCES product_variants(id),
  warehouse_id UUID NOT NULL,
  movement_type VARCHAR(50) NOT NULL,  -- purchase, return, adjustment, damage
  quantity INT NOT NULL,
  reference_id VARCHAR(50),  -- order_id, return_id
  notes TEXT,
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_variant_id (variant_id),
  INDEX idx_movement_type (movement_type)
);
```text
<!-- Code example in TEXT -->

### Orders & Fulfillment

```sql
<!-- Code example in SQL -->
-- Orders (customer purchases)
CREATE TABLE orders (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  order_number VARCHAR(20) UNIQUE NOT NULL,  -- Public facing
  customer_id UUID NOT NULL,
  status VARCHAR(50) NOT NULL,  -- pending, confirmed, processing, shipped, delivered, cancelled
  subtotal DECIMAL(12, 2),
  tax DECIMAL(10, 2),
  shipping_cost DECIMAL(10, 2),
  discount_amount DECIMAL(10, 2),
  total DECIMAL(12, 2) NOT NULL,
  currency VARCHAR(3) NOT NULL,  -- USD, EUR, etc.
  payment_status VARCHAR(50) NOT NULL,  -- pending, completed, failed, refunded
  billing_address_id UUID NOT NULL,
  shipping_address_id UUID NOT NULL,
  created_at TIMESTAMP DEFAULT NOW(),
  shipped_at TIMESTAMP,
  delivered_at TIMESTAMP,
  cancelled_at TIMESTAMP,

  INDEX idx_customer_id (customer_id),
  INDEX idx_status (status),
  INDEX idx_payment_status (payment_status),
  INDEX idx_created_at (created_at)
);

-- Order line items
CREATE TABLE order_items (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  variant_id UUID NOT NULL,
  quantity INT NOT NULL,
  unit_price DECIMAL(12, 2) NOT NULL,
  discount_amount DECIMAL(10, 2),
  total DECIMAL(12, 2) NOT NULL,

  INDEX idx_order_id (order_id)
);

-- Fulfillment operations
CREATE TABLE fulfillments (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  order_id UUID NOT NULL REFERENCES orders(id),
  status VARCHAR(50) NOT NULL,  -- pending, shipped, delivered, cancelled
  tracking_number VARCHAR(100),
  carrier VARCHAR(50),  -- FedEx, UPS, USPS
  estimated_delivery DATE,
  actual_delivery TIMESTAMP,
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_order_id (order_id),
  INDEX idx_status (status)
);

-- Fulfillment line items
CREATE TABLE fulfillment_items (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  fulfillment_id UUID NOT NULL REFERENCES fulfillments(id) ON DELETE CASCADE,
  order_item_id UUID NOT NULL REFERENCES order_items(id),
  quantity INT NOT NULL,

  INDEX idx_fulfillment_id (fulfillment_id)
);

-- Returns & Refunds
CREATE TABLE returns (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  order_id UUID NOT NULL REFERENCES orders(id),
  status VARCHAR(50) NOT NULL,  -- pending, approved, shipped, received, refunded
  reason VARCHAR(255),
  refund_amount DECIMAL(12, 2),
  refund_status VARCHAR(50) NOT NULL,  -- pending, completed, failed
  created_at TIMESTAMP DEFAULT NOW(),
  refunded_at TIMESTAMP,

  INDEX idx_order_id (order_id),
  INDEX idx_status (status)
);

-- Return line items
CREATE TABLE return_items (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  return_id UUID NOT NULL REFERENCES returns(id) ON DELETE CASCADE,
  order_item_id UUID NOT NULL REFERENCES order_items(id),
  quantity INT NOT NULL,
  condition VARCHAR(50),  -- unopened, opened, defective

  INDEX idx_return_id (return_id)
);
```text
<!-- Code example in TEXT -->

### Payments & Discounts

```sql
<!-- Code example in SQL -->
CREATE TABLE payments (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  order_id UUID NOT NULL REFERENCES orders(id),
  amount DECIMAL(12, 2) NOT NULL,
  status VARCHAR(50) NOT NULL,  -- pending, completed, failed, refunded
  payment_method VARCHAR(50),  -- credit_card, paypal, stripe
  transaction_id VARCHAR(100),
  error_message TEXT,
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_order_id (order_id),
  INDEX idx_status (status)
);

CREATE TABLE discounts (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  code VARCHAR(50) UNIQUE NOT NULL,
  type VARCHAR(50) NOT NULL,  -- percentage, fixed_amount
  value DECIMAL(10, 2) NOT NULL,
  max_uses INT,
  current_uses INT DEFAULT 0,
  min_order_amount DECIMAL(10, 2),
  valid_from DATE,
  valid_until DATE,
  is_active BOOLEAN DEFAULT TRUE,

  INDEX idx_code (code),
  INDEX idx_is_active (is_active)
);

CREATE TABLE order_discounts (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  order_id UUID NOT NULL REFERENCES orders(id),
  discount_id UUID NOT NULL REFERENCES discounts(id),
  discount_amount DECIMAL(10, 2),

  UNIQUE(order_id, discount_id)
);
```text
<!-- Code example in TEXT -->

---

## FraiseQL Schema

```python
<!-- Code example in Python -->
# ecommerce_schema.py
from FraiseQL import types, authorize
from decimal import Decimal
from datetime import datetime, date

@types.object
class Product:
    id: UUID  # UUID v4 for GraphQL ID
    sku: str
    name: str
    price: Decimal
    category: 'Category'
    variants: list['ProductVariant']
    inventory: list['Inventory']
    reviews: list['Review']
    rating: Decimal  # Average rating
    in_stock: bool

@types.object
class ProductVariant:
    id: UUID  # UUID v4 for GraphQL ID
    product: Product
    name: str
    price: Decimal
    sku: str
    weight: Decimal | None
    available_quantity: int

@types.object
class Order:
    id: UUID  # UUID v4 for GraphQL ID
    order_number: str
    customer: 'Customer'
    status: str
    items: list['OrderItem']
    subtotal: Decimal
    tax: Decimal
    shipping_cost: Decimal
    total: Decimal
    payment_status: str
    fulfillments: list['Fulfillment']
    returns: list['Return']
    created_at: datetime
    shipped_at: datetime | None

@types.object
class OrderItem:
    id: UUID  # UUID v4 for GraphQL ID
    variant: ProductVariant
    quantity: int
    unit_price: Decimal
    total: Decimal

@types.object
class Fulfillment:
    id: UUID  # UUID v4 for GraphQL ID
    order: Order
    status: str
    tracking_number: str | None
    carrier: str | None
    items: list['FulfillmentItem']
    estimated_delivery: date | None

@types.object
class Return:
    id: UUID  # UUID v4 for GraphQL ID
    order: Order
    status: str
    items: list['ReturnItem']
    refund_amount: Decimal
    refund_status: str
    reason: str

@types.object
class Query:
    def product(self, id: str) -> Product:
        """Get product details"""
        pass

    def search_products(
        self,
        query: str,
        category: str | None = None,
        min_price: Decimal | None = None,
        max_price: Decimal | None = None,
        limit: int = 50
    ) -> list[Product]:
        """Search products"""
        pass

    @authorize(roles=['customer', 'admin'])
    def my_orders(self, limit: int = 20) -> list[Order]:
        """Current user's orders"""
        pass

    @authorize(roles=['admin'])
    def orders(
        self,
        status: str | None = None,
        limit: int = 50
    ) -> list[Order]:
        """List orders (admin)"""
        pass

    @authorize(roles=['admin'])
    def inventory_status(self) -> dict:
        """Low stock alerts"""
        pass

@types.object
class Mutation:
    def add_to_cart(self, variant_id: str, quantity: int) -> 'CartItem':
        """Add product to cart"""
        pass

    @authorize(roles=['customer'])
    def create_order(
        self,
        cart_items: list[dict],
        shipping_address_id: str,
        discount_code: str | None = None
    ) -> Order:
        """Create order from cart"""
        pass

    @authorize(roles=['customer'])
    def cancel_order(self, order_id: str) -> Order:
        """Cancel pending order"""
        pass

    @authorize(roles=['customer'])
    def return_items(
        self,
        order_id: str,
        items: list[dict]
    ) -> Return:
        """Initiate return"""
        pass

    @authorize(roles=['admin'])
    def create_fulfillment(
        self,
        order_id: str,
        items: list[dict],
        carrier: str,
        tracking_number: str
    ) -> Fulfillment:
        """Create shipment"""
        pass

    @authorize(roles=['admin'])
    def process_payment(self, order_id: str) -> 'Payment':
        """Process order payment"""
        pass
```text
<!-- Code example in TEXT -->

---

## Order Lifecycle

### State Machine

**Diagram:** System architecture visualization

```d2
<!-- Code example in D2 Diagram -->
direction: down

Pending: "pending\n(awaiting payment)" {
  shape: box
  style.fill: "#fff9c4"
}

Confirmed: "confirmed\n(payment received)" {
  shape: box
  style.fill: "#fff3e0"
}

Processing: "processing\n(preparing shipment)" {
  shape: box
  style.fill: "#ffe0b2"
}

Shipped: "shipped\n(in transit)" {
  shape: box
  style.fill: "#ffccbc"
}

Delivered: "delivered\n(final destination)" {
  shape: box
  style.fill: "#c8e6c9"
}

Cancelled: "cancelled\n(at any point)" {
  shape: box
  style.fill: "#ffebee"
}

Pending -> Confirmed
Confirmed -> Processing
Processing -> Shipped
Shipped -> Delivered
Pending -> Cancelled
Confirmed -> Cancelled
Processing -> Cancelled
Shipped -> Cancelled
```text
<!-- Code example in TEXT -->

### State Transitions

```sql
<!-- Code example in SQL -->
-- Trigger to enforce state transitions
CREATE OR REPLACE FUNCTION validate_order_transition()
RETURNS TRIGGER AS $$
BEGIN
  -- Valid transitions
  CASE
    WHEN OLD.status = 'pending' AND NEW.status NOT IN ('confirmed', 'cancelled') THEN
      RAISE EXCEPTION 'Invalid transition from pending';
    WHEN OLD.status = 'confirmed' AND NEW.status NOT IN ('processing', 'cancelled') THEN
      RAISE EXCEPTION 'Invalid transition from confirmed';
    WHEN OLD.status = 'shipped' AND NEW.status NOT IN ('delivered', 'cancelled') THEN
      RAISE EXCEPTION 'Invalid transition from shipped';
    WHEN OLD.status IN ('delivered', 'cancelled') THEN
      RAISE EXCEPTION 'Cannot change delivered or cancelled orders';
  END CASE;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER order_status_validation
BEFORE UPDATE ON orders
FOR EACH ROW
WHEN (OLD.status IS DISTINCT FROM NEW.status)
EXECUTE FUNCTION validate_order_transition();
```text
<!-- Code example in TEXT -->

---

## Inventory Management

### Reserve on Order Creation

```sql
<!-- Code example in SQL -->
CREATE OR REPLACE FUNCTION reserve_inventory()
RETURNS TRIGGER AS $$
BEGIN
  -- Check available inventory
  UPDATE inventory
  SET quantity_reserved = quantity_reserved + NEW.quantity
  WHERE variant_id = NEW.variant_id
    AND quantity_available >= NEW.quantity;

  IF NOT FOUND THEN
    RAISE EXCEPTION 'Insufficient inventory for variant %', NEW.variant_id;
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER order_item_reserve
BEFORE INSERT ON order_items
FOR EACH ROW
EXECUTE FUNCTION reserve_inventory();
```text
<!-- Code example in TEXT -->

### Release on Cancellation

```sql
<!-- Code example in SQL -->
CREATE OR REPLACE FUNCTION release_inventory()
RETURNS TRIGGER AS $$
BEGIN
  UPDATE inventory
  SET quantity_reserved = quantity_reserved - oi.quantity
  FROM order_items oi
  WHERE inventory.variant_id = oi.variant_id
    AND oi.order_id = NEW.id;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER order_cancel_release
BEFORE UPDATE ON orders
FOR EACH ROW
WHEN (OLD.status != 'cancelled' AND NEW.status = 'cancelled')
EXECUTE FUNCTION release_inventory();
```text
<!-- Code example in TEXT -->

---

## Payment Processing

```typescript
<!-- Code example in TypeScript -->
const PROCESS_PAYMENT = gql`
  mutation ProcessPayment($orderId: ID!, $paymentMethod: String!) {
    processPayment(orderId: $orderId, paymentMethod: $paymentMethod) {
      id
      status
      order {
        id
        payment_status
      }
    }
  }
`;

export async function processOrderPayment(
  orderId: string,
  token: string
) {
  try {
    // Charge customer via Stripe/PayPal
    const charge = await stripe.charges.create({
      amount: order.total * 100, // Convert to cents
      currency: order.currency.toLowerCase(),
      source: token,
      metadata: { orderId },
    });

    // Record payment in database
    const result = await client.mutation(PROCESS_PAYMENT, {
      variables: {
        orderId,
        paymentMethod: 'stripe',
      },
    });

    // Update order status
    if (result.data.processPayment.status === 'completed') {
      await client.mutation(UPDATE_ORDER_STATUS, {
        variables: {
          orderId,
          status: 'confirmed',
        },
      });
    }

    return result;
  } catch (error) {
    // Handle payment failure
    console.error('Payment failed:', error);
    throw error;
  }
}
```text
<!-- Code example in TEXT -->

---

## Reporting & Analytics

```typescript
<!-- Code example in TypeScript -->
const ORDER_METRICS = gql`
  query OrderMetrics($startDate: Date!, $endDate: Date!) {
    orderMetrics(startDate: $startDate, endDate: $endDate) {
      total_revenue
      order_count
      average_order_value
      conversion_rate
      top_products {
        id
        name
        revenue
        quantity_sold
      }
    }
  }
`;

export function RevenueReport() {
  const [dateRange, setDateRange] = useState({
    start: subDays(new Date(), 30),
    end: new Date(),
  });

  const { data } = useQuery(ORDER_METRICS, {
    variables: {
      startDate: format(dateRange.start, 'yyyy-MM-dd'),
      endDate: format(dateRange.end, 'yyyy-MM-dd'),
    },
  });

  return (
    <div className="report">
      <h1>Revenue Report</h1>
      <KPI label="Revenue" value={formatCurrency(data?.orderMetrics?.total_revenue)} />
      <KPI label="Orders" value={data?.orderMetrics?.order_count} />
      <KPI label="AOV" value={formatCurrency(data?.orderMetrics?.average_order_value)} />
      <TopProducts products={data?.orderMetrics?.top_products} />
    </div>
  );
}
```text
<!-- Code example in TEXT -->

---

## Testing Order Workflows

```typescript
<!-- Code example in TypeScript -->
describe('Order Management', () => {
  it('should create order and reserve inventory', async () => {
    const variant = await createVariant();
    const initialStock = 100;
    await setInventory(variant.id, initialStock);

    const order = await createOrder([
      { variantId: variant.id, quantity: 10 }
    ]);

    const inventory = await getInventory(variant.id);
    expect(inventory.quantityReserved).toBe(10);
    expect(inventory.quantityAvailable).toBe(90);
  });

  it('should prevent overselling', async () => {
    const variant = await createVariant();
    await setInventory(variant.id, 5);

    const createOrder = async () => {
      return await createOrder([
        { variantId: variant.id, quantity: 10 }
      ]);
    };

    expect(createOrder()).rejects.toThrow('Insufficient inventory');
  });

  it('should release inventory on cancellation', async () => {
    const order = await createOrder([...]);
    const inventory = await getInventory(variant.id);
    const reserved = inventory.quantityReserved;

    await cancelOrder(order.id);

    const updatedInventory = await getInventory(variant.id);
    expect(updatedInventory.quantityReserved).toBe(reserved - 10);
  });
});
```text
<!-- Code example in TEXT -->

---

## See Also

**Related Patterns:**

- [Multi-Tenant SaaS](./saas-multi-tenant.md) - Multi-vendor marketplaces
- [Analytics Platform](./analytics-olap-platform.md) - Sales reporting

**Guides:**

- [Production Deployment](../guides/production-deployment.md)
- [Schema Design Best Practices](../guides/schema-design-best-practices.md)

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
