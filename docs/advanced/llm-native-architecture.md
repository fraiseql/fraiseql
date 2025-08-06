# LLM-Native Architecture

FraiseQL's database-first approach creates APIs that are inherently compatible with Large Language Models. By designing self-documenting schemas, predictable patterns, and AI-friendly interfaces, FraiseQL enables seamless integration with AI development workflows and autonomous code generation.

## Architectural Philosophy

### AI as a First-Class Developer

FraiseQL treats AI systems as primary API consumers, not secondary integrations. This means:

1. **Predictable Patterns**: Consistent naming and structure reduce hallucination
2. **Self-Documenting**: Database comments and metadata provide context
3. **Type Safety**: Strong typing prevents AI-generated errors
4. **Introspection**: Full schema discovery enables autonomous exploration
5. **Natural Language Mapping**: Database schema mirrors business concepts

### Human + AI Collaboration

The architecture supports both human developers and AI systems working together:
- Humans design the domain model and business rules
- AI generates queries, mutations, and client code
- Database enforces correctness regardless of who wrote the code

## Self-Documenting Schemas

### Rich Comments for AI Understanding

Use PostgreSQL comments to provide context for AI systems:

```sql
-- Domain-rich table comments
CREATE TABLE tb_subscription_plans (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    monthly_price NUMERIC(8,2) NOT NULL,
    yearly_price NUMERIC(8,2),
    features JSONB NOT NULL,
    max_users INT,
    max_storage_gb INT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON TABLE tb_subscription_plans IS
'Subscription plans define pricing tiers and feature access for SaaS customers.
Plans can be billed monthly or yearly with different feature limits.';

COMMENT ON COLUMN tb_subscription_plans.features IS
'JSONB object defining available features like {"api_access": true, "priority_support": false, "custom_branding": true}';

COMMENT ON COLUMN tb_subscription_plans.max_users IS
'Maximum number of user accounts allowed on this plan. NULL means unlimited.';

COMMENT ON COLUMN tb_subscription_plans.max_storage_gb IS
'Storage limit in gigabytes. NULL means unlimited storage.';

-- AI-friendly view with comprehensive documentation
CREATE OR REPLACE VIEW v_subscription_plans AS
SELECT
    p.id,
    p.name,  -- For filtering and searching
    p.monthly_price,  -- For price range queries
    jsonb_build_object(
        '__typename', 'SubscriptionPlan',
        'id', p.id,
        'name', p.name,
        'pricing', jsonb_build_object(
            'monthly', p.monthly_price,
            'yearly', p.yearly_price,
            'yearly_discount', CASE
                WHEN p.yearly_price IS NOT NULL AND p.monthly_price > 0
                THEN ROUND(((p.monthly_price * 12 - p.yearly_price) / (p.monthly_price * 12)) * 100, 1)
                ELSE NULL
            END
        ),
        'features', p.features,
        'limits', jsonb_build_object(
            'max_users', p.max_users,
            'max_storage_gb', p.max_storage_gb,
            'unlimited_users', p.max_users IS NULL,
            'unlimited_storage', p.max_storage_gb IS NULL
        ),
        'created_at', p.created_at
    ) AS data
FROM tb_subscription_plans p
WHERE p.monthly_price > 0;  -- Only active plans

COMMENT ON VIEW v_subscription_plans IS
'Subscription plans available for purchase. Includes calculated yearly discount percentage
and normalized limits structure. Use this view for plan selection UI and billing flows.
Example queries:
- Find plans under $50/month: WHERE monthly_price < 50
- Find plans with API access: WHERE (data->''features''->>''api_access'')::boolean = true
- Find unlimited plans: WHERE (data->''limits''->>''unlimited_users'')::boolean = true';
```

### Semantic Field Names

Choose field names that clearly express business intent:

```sql
-- Bad: Generic, unclear names
CREATE TABLE tb_items (
    id UUID,
    type INT,  -- What type? Status? Category?
    value NUMERIC,  -- Value of what?
    flag BOOLEAN  -- What flag?
);

-- Good: Business-meaningful names
CREATE TABLE tb_invoice_line_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID NOT NULL REFERENCES tb_invoices(id),
    product_name VARCHAR(200) NOT NULL,  -- Clear what this represents
    unit_price NUMERIC(10,2) NOT NULL,   -- Price per unit
    quantity INT NOT NULL DEFAULT 1,     -- How many units
    discount_percent NUMERIC(5,2) DEFAULT 0,  -- Discount percentage
    tax_rate NUMERIC(5,4) NOT NULL,      -- Tax rate (e.g., 0.0825 for 8.25%)
    is_taxable BOOLEAN DEFAULT true,     -- Whether this item is taxable
    line_total NUMERIC(10,2) GENERATED ALWAYS AS (
        (unit_price * quantity * (1 - discount_percent/100)) *
        (1 + CASE WHEN is_taxable THEN tax_rate ELSE 0 END)
    ) STORED
);

COMMENT ON TABLE tb_invoice_line_items IS
'Individual line items on invoices. Each line represents a product or service
with pricing, tax, and discount calculations. Line total is automatically
calculated including discounts and tax.';

COMMENT ON COLUMN tb_invoice_line_items.line_total IS
'Calculated total for this line item: (unit_price * quantity * (1 - discount_percent/100)) * (1 + tax_rate if taxable)';
```

### AI Query Examples in Comments

Provide example queries in view comments to guide AI systems:

```sql
CREATE OR REPLACE VIEW v_customer_analytics AS
SELECT
    c.id,
    c.email,  -- For customer lookup
    c.created_at,  -- For cohort analysis
    jsonb_build_object(
        '__typename', 'CustomerAnalytics',
        'customer', jsonb_build_object(
            'id', c.id,
            'email', c.email,
            'name', c.name,
            'company', c.company
        ),
        'metrics', jsonb_build_object(
            'lifetime_value', COALESCE(SUM(i.total_amount), 0),
            'total_invoices', COUNT(i.id),
            'avg_invoice_value', COALESCE(AVG(i.total_amount), 0),
            'first_purchase', MIN(i.created_at),
            'last_purchase', MAX(i.created_at),
            'days_since_last_purchase', CASE
                WHEN MAX(i.created_at) IS NOT NULL
                THEN EXTRACT(days FROM NOW() - MAX(i.created_at))::INT
                ELSE NULL
            END,
            'payment_methods_used', COUNT(DISTINCT p.payment_method),
            'most_common_payment', MODE() WITHIN GROUP (ORDER BY p.payment_method)
        ),
        'segments', (
            SELECT ARRAY_AGG(DISTINCT segment)
            FROM (
                SELECT
                    CASE
                        WHEN COALESCE(SUM(i2.total_amount), 0) > 10000 THEN 'high_value'
                        WHEN COALESCE(SUM(i2.total_amount), 0) > 1000 THEN 'medium_value'
                        WHEN COALESCE(SUM(i2.total_amount), 0) > 0 THEN 'low_value'
                        ELSE 'prospect'
                    END AS segment
                FROM tb_invoices i2
                WHERE i2.customer_id = c.id
                UNION
                SELECT
                    CASE
                        WHEN COUNT(i3.id) >= 12 THEN 'frequent_buyer'
                        WHEN COUNT(i3.id) >= 3 THEN 'regular_buyer'
                        WHEN COUNT(i3.id) >= 1 THEN 'occasional_buyer'
                        ELSE 'new_prospect'
                    END AS segment
                FROM tb_invoices i3
                WHERE i3.customer_id = c.id
            ) segments
        )
    ) AS data
FROM tb_customers c
LEFT JOIN tb_invoices i ON i.customer_id = c.id
LEFT JOIN tb_payments p ON p.invoice_id = i.id
GROUP BY c.id, c.email, c.name, c.company, c.created_at;

COMMENT ON VIEW v_customer_analytics IS
'Comprehensive customer analytics including lifetime value, purchase patterns, and automatic segmentation.

AI Query Examples:
- Find high-value customers: WHERE (data->''metrics''->>''lifetime_value'')::numeric > 10000
- Find recent customers: WHERE created_at > NOW() - INTERVAL ''30 days''
- Find customers who haven''t purchased recently: WHERE (data->''metrics''->>''days_since_last_purchase'')::int > 90
- Find frequent buyers: WHERE data->''segments'' ? ''frequent_buyer''
- Find customers by payment method: WHERE (data->''metrics''->>''most_common_payment'') = ''credit_card''
- Customer cohort analysis: GROUP BY DATE_TRUNC(''month'', created_at)

Business Use Cases:
- Customer segmentation for marketing campaigns
- Churn prediction (customers with high days_since_last_purchase)
- Lifetime value optimization
- Payment method analysis
- Customer onboarding analysis';
```

## Predictable Query Patterns

### Consistent WHERE Clause Structure

Establish standard patterns for filtering that AI can learn:

```sql
-- Standard filtering patterns for all entity views
CREATE OR REPLACE VIEW v_orders_filterable AS
SELECT
    o.id,
    o.customer_id,      -- For customer-specific filtering
    o.status,           -- For status filtering
    o.created_at,       -- For date range filtering
    o.total_amount,     -- For amount range filtering
    o.payment_status,   -- For payment status filtering
    jsonb_build_object(
        '__typename', 'Order',
        'id', o.id,
        'order_number', 'ORD-' || LPAD(o.id::text, 8, '0'),
        'customer', c.data,
        'status', o.status,
        'payment_status', o.payment_status,
        'total_amount', o.total_amount,
        'currency', o.currency,
        'items', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'product_name', oi.product_name,
                    'quantity', oi.quantity,
                    'unit_price', oi.unit_price,
                    'line_total', oi.line_total
                ) ORDER BY oi.created_at
            )
            FROM tb_order_items oi
            WHERE oi.order_id = o.id
        ),
        'created_at', o.created_at,
        'updated_at', o.updated_at
    ) AS data
FROM tb_orders o
LEFT JOIN v_customers c ON c.id = o.customer_id;

COMMENT ON VIEW v_orders_filterable IS
'Orders with consistent filtering patterns. All filter columns are exposed for standard queries.

Standard Filter Patterns:
- By customer: WHERE customer_id = $1
- By status: WHERE status = $1 or WHERE status IN ($1, $2, $3)
- By date range: WHERE created_at >= $1 AND created_at <= $2
- By amount range: WHERE total_amount >= $1 AND total_amount <= $2
- By payment status: WHERE payment_status = $1
- Recent orders: WHERE created_at > NOW() - INTERVAL ''7 days''
- Large orders: WHERE total_amount > 1000
- Pending orders: WHERE status = ''pending'' AND payment_status = ''unpaid''

AI can combine these patterns:
WHERE customer_id = $1 AND status IN (''pending'', ''processing'') AND created_at >= $2';
```

### Standard Aggregation Patterns

Provide common aggregation examples:

```sql
-- Standard aggregation view with examples
CREATE OR REPLACE VIEW v_sales_aggregations AS
SELECT
    'daily' as period_type,
    DATE(created_at) as period,
    jsonb_build_object(
        '__typename', 'SalesAggregation',
        'period', DATE(created_at),
        'period_type', 'daily',
        'metrics', jsonb_build_object(
            'total_orders', COUNT(*),
            'total_revenue', SUM(total_amount),
            'avg_order_value', AVG(total_amount),
            'unique_customers', COUNT(DISTINCT customer_id),
            'new_customers', COUNT(DISTINCT CASE
                WHEN NOT EXISTS (
                    SELECT 1 FROM tb_orders o2
                    WHERE o2.customer_id = tb_orders.customer_id
                    AND o2.created_at < tb_orders.created_at
                ) THEN customer_id
            END)
        )
    ) AS data
FROM tb_orders
WHERE status = 'completed'
GROUP BY DATE(created_at)

UNION ALL

SELECT
    'weekly' as period_type,
    DATE_TRUNC('week', created_at) as period,
    jsonb_build_object(
        '__typename', 'SalesAggregation',
        'period', DATE_TRUNC('week', created_at),
        'period_type', 'weekly',
        'metrics', jsonb_build_object(
            'total_orders', COUNT(*),
            'total_revenue', SUM(total_amount),
            'avg_order_value', AVG(total_amount),
            'unique_customers', COUNT(DISTINCT customer_id),
            'new_customers', COUNT(DISTINCT CASE
                WHEN NOT EXISTS (
                    SELECT 1 FROM tb_orders o2
                    WHERE o2.customer_id = tb_orders.customer_id
                    AND o2.created_at < DATE_TRUNC('week', tb_orders.created_at)
                ) THEN customer_id
            END)
        )
    ) AS data
FROM tb_orders
WHERE status = 'completed'
GROUP BY DATE_TRUNC('week', created_at);

COMMENT ON VIEW v_sales_aggregations IS
'Sales metrics aggregated by time period. Supports daily and weekly aggregations.

AI Query Patterns:
- Last 30 days daily: WHERE period_type = ''daily'' AND period >= CURRENT_DATE - 30
- This week: WHERE period_type = ''weekly'' AND period = DATE_TRUNC(''week'', CURRENT_DATE)
- Revenue trend: ORDER BY period to see growth over time
- Peak periods: ORDER BY (data->''metrics''->>''total_revenue'')::numeric DESC
- Customer acquisition: Focus on new_customers metric

Combine with other filters:
- High revenue days: WHERE (data->''metrics''->>''total_revenue'')::numeric > 10000
- Low order days: WHERE (data->''metrics''->>''total_orders'')::int < 10';
```

## Natural Language to GraphQL Translation

### Semantic Field Mapping

Design GraphQL fields that match natural language concepts:

```sql
-- Database view designed for natural language queries
CREATE OR REPLACE VIEW v_products_nlp_friendly AS
SELECT
    p.id,
    p.name,
    p.category_id,
    p.base_price,
    p.created_at,
    jsonb_build_object(
        '__typename', 'Product',
        'id', p.id,
        'name', p.name,
        'description', p.description,
        -- Natural language friendly fields
        'price', jsonb_build_object(
            'amount', p.base_price,
            'currency', 'USD',
            'formatted', '$' || p.base_price::text,
            'is_expensive', p.base_price > 100,
            'is_budget_friendly', p.base_price <= 50,
            'price_tier', CASE
                WHEN p.base_price <= 25 THEN 'budget'
                WHEN p.base_price <= 100 THEN 'mid_range'
                ELSE 'premium'
            END
        ),
        'availability', jsonb_build_object(
            'in_stock', (
                SELECT SUM(quantity) > 0
                FROM tb_inventory
                WHERE product_id = p.id
            ),
            'stock_level', (
                SELECT
                    CASE
                        WHEN SUM(quantity) > 100 THEN 'high'
                        WHEN SUM(quantity) > 10 THEN 'medium'
                        WHEN SUM(quantity) > 0 THEN 'low'
                        ELSE 'out_of_stock'
                    END
                FROM tb_inventory
                WHERE product_id = p.id
            ),
            'estimated_restock', CASE
                WHEN (SELECT SUM(quantity) FROM tb_inventory WHERE product_id = p.id) > 0
                THEN NULL
                ELSE CURRENT_DATE + INTERVAL '2 weeks'
            END
        ),
        'popularity', jsonb_build_object(
            'view_count', (
                SELECT COUNT(*) FROM tb_product_views
                WHERE product_id = p.id
                AND created_at > CURRENT_DATE - INTERVAL '30 days'
            ),
            'purchase_count', (
                SELECT COUNT(*) FROM tb_order_items oi
                JOIN tb_orders o ON o.id = oi.order_id
                WHERE oi.product_id = p.id
                AND o.status = 'completed'
                AND o.created_at > CURRENT_DATE - INTERVAL '30 days'
            ),
            'is_trending', (
                SELECT COUNT(*) > 10 FROM tb_product_views
                WHERE product_id = p.id
                AND created_at > CURRENT_DATE - INTERVAL '7 days'
            ),
            'is_bestseller', (
                SELECT COUNT(*) FROM tb_order_items oi
                JOIN tb_orders o ON o.id = oi.order_id
                WHERE oi.product_id = p.id
                AND o.status = 'completed'
                AND o.created_at > CURRENT_DATE - INTERVAL '30 days'
            ) >= 100
        ),
        'category', (SELECT data FROM v_categories WHERE id = p.category_id),
        'created_at', p.created_at
    ) AS data
FROM tb_products p
WHERE p.status = 'active';

COMMENT ON VIEW v_products_nlp_friendly IS
'Product catalog optimized for natural language queries and AI understanding.

Natural Language Query Mappings:
- "expensive products" → WHERE (data->''price''->>''is_expensive'')::boolean = true
- "budget products" → WHERE (data->''price''->>''is_budget_friendly'')::boolean = true
- "out of stock items" → WHERE data->''availability''->>''stock_level'' = ''out_of_stock''
- "trending products" → WHERE (data->''popularity''->>''is_trending'')::boolean = true
- "bestselling items" → WHERE (data->''popularity''->>''is_bestseller'')::boolean = true
- "premium products" → WHERE data->''price''->>''price_tier'' = ''premium''
- "popular this month" → WHERE (data->''popularity''->>''view_count'')::int > 50

AI can understand and generate these queries from natural language inputs.';
```

### Query Intent Recognition

Structure data to support common query intents:

```sql
-- Intent-based view for common business questions
CREATE OR REPLACE VIEW v_business_insights AS
SELECT
    'customer_behavior' as insight_category,
    'churn_risk' as insight_type,
    c.id as entity_id,
    c.email as entity_name,
    jsonb_build_object(
        '__typename', 'BusinessInsight',
        'category', 'customer_behavior',
        'type', 'churn_risk',
        'entity', jsonb_build_object(
            'id', c.id,
            'name', c.name,
            'email', c.email
        ),
        'risk_score', CASE
            WHEN days_since_last_order > 90 AND lifetime_orders > 3 THEN 'high'
            WHEN days_since_last_order > 60 AND lifetime_orders > 1 THEN 'medium'
            WHEN days_since_last_order > 30 AND lifetime_orders > 0 THEN 'low'
            ELSE 'none'
        END,
        'metrics', jsonb_build_object(
            'days_since_last_order', days_since_last_order,
            'lifetime_orders', lifetime_orders,
            'lifetime_value', lifetime_value,
            'avg_order_value', CASE WHEN lifetime_orders > 0 THEN lifetime_value / lifetime_orders ELSE 0 END
        ),
        'recommendations', CASE
            WHEN days_since_last_order > 90 THEN jsonb_build_array(
                'Send personalized discount offer',
                'Re-engagement email campaign',
                'Check product recommendations'
            )
            WHEN days_since_last_order > 60 THEN jsonb_build_array(
                'Send product update newsletter',
                'Offer limited-time promotion'
            )
            ELSE jsonb_build_array()
        END
    ) AS data
FROM (
    SELECT
        c.id,
        c.name,
        c.email,
        COALESCE(EXTRACT(days FROM NOW() - MAX(o.created_at))::int, 999) as days_since_last_order,
        COUNT(o.id) as lifetime_orders,
        COALESCE(SUM(o.total_amount), 0) as lifetime_value
    FROM tb_customers c
    LEFT JOIN tb_orders o ON o.customer_id = c.id AND o.status = 'completed'
    GROUP BY c.id, c.name, c.email
) customer_stats
WHERE lifetime_orders > 0;  -- Only customers who have purchased

COMMENT ON VIEW v_business_insights IS
'Business insights derived from customer behavior patterns. Designed for AI to answer natural language business questions.

Natural Language Questions Supported:
- "Which customers are at risk of churning?" → WHERE data->''risk_score'' IN (''high'', ''medium'')
- "Who are our high-value customers at risk?" → WHERE data->''risk_score'' = ''high'' AND (data->''metrics''->>''lifetime_value'')::numeric > 1000
- "What should we do about churning customers?" → Look at data->''recommendations''
- "Show me customers who haven''t ordered in 3 months" → WHERE (data->''metrics''->>''days_since_last_order'')::int > 90

This view transforms raw data into actionable business insights that AI can interpret and act upon.';
```

## Type Safety for AI

### Strong Typing Prevents Errors

Use PostgreSQL's type system to prevent common AI mistakes:

```sql
-- Strongly typed enums prevent invalid values
CREATE TYPE order_status AS ENUM (
    'pending',
    'confirmed',
    'processing',
    'shipped',
    'delivered',
    'cancelled',
    'refunded'
);

CREATE TYPE payment_status AS ENUM (
    'pending',
    'authorized',
    'paid',
    'failed',
    'refunded',
    'disputed'
);

-- Table with strong typing
CREATE TABLE tb_orders_typed (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id UUID NOT NULL REFERENCES tb_customers(id),
    status order_status NOT NULL DEFAULT 'pending',
    payment_status payment_status NOT NULL DEFAULT 'pending',
    total_amount NUMERIC(10,2) NOT NULL CHECK (total_amount >= 0),
    currency CHAR(3) NOT NULL DEFAULT 'USD' CHECK (currency ~ '^[A-Z]{3}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    shipped_at TIMESTAMPTZ CHECK (shipped_at IS NULL OR shipped_at >= created_at),
    delivered_at TIMESTAMPTZ CHECK (delivered_at IS NULL OR delivered_at >= shipped_at)
);

COMMENT ON TYPE order_status IS
'Valid order status values. AI systems should only use these exact values when updating orders.';

COMMENT ON TYPE payment_status IS
'Valid payment status values. These track the payment lifecycle from pending to final status.';

-- View with type validation examples
CREATE OR REPLACE VIEW v_orders_typed AS
SELECT
    o.id,
    o.customer_id,
    o.status,  -- Exposed as filter column
    o.payment_status,  -- Exposed as filter column
    o.total_amount,  -- Exposed as filter column
    jsonb_build_object(
        '__typename', 'Order',
        'id', o.id,
        'status', o.status,
        'payment_status', o.payment_status,
        'total_amount', o.total_amount,
        'currency', o.currency,
        'status_info', jsonb_build_object(
            'can_cancel', o.status IN ('pending', 'confirmed'),
            'can_ship', o.status = 'processing' AND o.payment_status = 'paid',
            'can_refund', o.status IN ('delivered', 'shipped') AND o.payment_status = 'paid',
            'is_completed', o.status = 'delivered',
            'requires_payment', o.payment_status IN ('pending', 'failed')
        ),
        'dates', jsonb_build_object(
            'created_at', o.created_at,
            'shipped_at', o.shipped_at,
            'delivered_at', o.delivered_at,
            'estimated_delivery', CASE
                WHEN o.shipped_at IS NOT NULL THEN o.shipped_at + INTERVAL '3 days'
                WHEN o.status = 'processing' THEN NOW() + INTERVAL '5 days'
                ELSE NULL
            END
        )
    ) AS data
FROM tb_orders_typed o;

COMMENT ON VIEW v_orders_typed IS
'Strongly typed orders view with validation helpers. AI can safely query this without type errors.

Type-Safe Query Examples:
- Valid statuses only: WHERE status = ''pending'' (will error if invalid status used)
- Status transitions: Use status_info to check what operations are allowed
- Date validation: All date constraints are enforced at database level
- Amount validation: total_amount is guaranteed to be >= 0

AI Benefits:
- Cannot insert invalid enum values
- Cannot set negative amounts
- Cannot set delivered_at before shipped_at
- Clear status transition rules in status_info';
```

### Domain Constraints Guide AI

Use database constraints to encode business rules:

```sql
-- Business rule constraints that guide AI behavior
CREATE TABLE tb_subscription_changes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id UUID NOT NULL REFERENCES tb_customers(id),
    from_plan_id UUID REFERENCES tb_subscription_plans(id),
    to_plan_id UUID NOT NULL REFERENCES tb_subscription_plans(id),
    change_type VARCHAR(20) NOT NULL CHECK (change_type IN ('upgrade', 'downgrade', 'switch')),
    effective_date DATE NOT NULL DEFAULT CURRENT_DATE,
    prorated_credit NUMERIC(8,2) DEFAULT 0 CHECK (prorated_credit >= 0),
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Business rule: Can't change to the same plan
    CONSTRAINT no_same_plan_change CHECK (from_plan_id != to_plan_id),

    -- Business rule: Effective date can't be in the past
    CONSTRAINT effective_date_not_past CHECK (effective_date >= CURRENT_DATE),

    -- Business rule: Can't have multiple changes for same customer on same date
    CONSTRAINT one_change_per_customer_per_day UNIQUE (customer_id, effective_date)
);

-- Function that AI can safely call
CREATE OR REPLACE FUNCTION fn_change_subscription_plan(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_customer_id UUID;
    v_from_plan_id UUID;
    v_to_plan_id UUID;
    v_from_plan RECORD;
    v_to_plan RECORD;
    v_change_type TEXT;
    v_prorated_credit NUMERIC;
BEGIN
    -- Extract and validate input
    v_customer_id := (input_data->>'customer_id')::UUID;
    v_to_plan_id := (input_data->>'to_plan_id')::UUID;

    -- Get customer's current plan
    SELECT sp.*, cs.plan_id INTO v_from_plan
    FROM tb_customer_subscriptions cs
    JOIN tb_subscription_plans sp ON sp.id = cs.plan_id
    WHERE cs.customer_id = v_customer_id
        AND cs.status = 'active'
    LIMIT 1;

    IF NOT FOUND THEN
        RETURN ROW(false, 'Customer has no active subscription', NULL)::mutation_result;
    END IF;

    v_from_plan_id := v_from_plan.id;

    -- Get target plan details
    SELECT * INTO v_to_plan
    FROM tb_subscription_plans
    WHERE id = v_to_plan_id;

    IF NOT FOUND THEN
        RETURN ROW(false, 'Target plan not found', NULL)::mutation_result;
    END IF;

    -- Determine change type and calculate prorated credit
    IF v_to_plan.monthly_price > v_from_plan.monthly_price THEN
        v_change_type := 'upgrade';
        v_prorated_credit := 0;  -- Customer pays more
    ELSIF v_to_plan.monthly_price < v_from_plan.monthly_price THEN
        v_change_type := 'downgrade';
        -- Calculate prorated credit for remaining time
        v_prorated_credit := (v_from_plan.monthly_price - v_to_plan.monthly_price) * 0.5;  -- Simplified
    ELSE
        v_change_type := 'switch';  -- Same price, different features
        v_prorated_credit := 0;
    END IF;

    -- Create subscription change record (constraints will validate)
    INSERT INTO tb_subscription_changes (
        customer_id,
        from_plan_id,
        to_plan_id,
        change_type,
        prorated_credit,
        effective_date
    ) VALUES (
        v_customer_id,
        v_from_plan_id,
        v_to_plan_id,
        v_change_type,
        v_prorated_credit,
        COALESCE((input_data->>'effective_date')::DATE, CURRENT_DATE)
    );

    -- Apply the change immediately if effective today
    IF COALESCE((input_data->>'effective_date')::DATE, CURRENT_DATE) = CURRENT_DATE THEN
        UPDATE tb_customer_subscriptions
        SET plan_id = v_to_plan_id,
            updated_at = NOW()
        WHERE customer_id = v_customer_id
            AND status = 'active';
    END IF;

    RETURN ROW(
        true,
        'Subscription change scheduled successfully',
        jsonb_build_object(
            'change_type', v_change_type,
            'from_plan', v_from_plan.name,
            'to_plan', v_to_plan.name,
            'prorated_credit', v_prorated_credit,
            'effective_date', COALESCE((input_data->>'effective_date')::DATE, CURRENT_DATE)
        )
    )::mutation_result;

EXCEPTION
    WHEN unique_violation THEN
        RETURN ROW(false, 'Customer already has a plan change scheduled for this date', NULL)::mutation_result;
    WHEN check_violation THEN
        RETURN ROW(false, 'Invalid plan change: ' || SQLERRM, NULL)::mutation_result;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION fn_change_subscription_plan IS
'Safely changes customer subscription plans with automatic validation and business rule enforcement.

AI Usage:
- All business rules are enforced by database constraints
- Function handles upgrade/downgrade logic automatically
- Cannot create invalid changes (same plan, past dates, etc.)
- Returns clear success/error messages for AI to interpret

Input format: {"customer_id": "uuid", "to_plan_id": "uuid", "effective_date": "2024-01-15"}';
```

## Prompt-Friendly API Design

### Standardized Response Formats

Design consistent response formats that AI can predict:

```sql
-- Standardized error responses
CREATE OR REPLACE FUNCTION standardized_error(
    p_code TEXT,
    p_message TEXT,
    p_details JSONB DEFAULT NULL
) RETURNS mutation_result AS $$
BEGIN
    RETURN ROW(
        false,  -- success = false
        p_message,
        jsonb_build_object(
            'error', jsonb_build_object(
                'code', p_code,
                'message', p_message,
                'details', COALESCE(p_details, '{}'::jsonb),
                'timestamp', NOW(),
                'type', 'validation_error'
            )
        )
    )::mutation_result;
END;
$$ LANGUAGE plpgsql;

-- Standardized success responses
CREATE OR REPLACE FUNCTION standardized_success(
    p_message TEXT,
    p_data JSONB
) RETURNS mutation_result AS $$
BEGIN
    RETURN ROW(
        true,  -- success = true
        p_message,
        jsonb_build_object(
            'result', p_data,
            'success', true,
            'timestamp', NOW()
        )
    )::mutation_result;
END;
$$ LANGUAGE plpgsql;

-- Example function using standardized responses
CREATE OR REPLACE FUNCTION fn_create_customer(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_customer_id UUID;
    v_email TEXT;
BEGIN
    -- Extract email
    v_email := input_data->>'email';

    -- Validate email format
    IF v_email !~ '^[^@]+@[^@]+\.[^@]+$' THEN
        RETURN standardized_error(
            'INVALID_EMAIL',
            'Invalid email address format',
            jsonb_build_object('field', 'email', 'value', v_email)
        );
    END IF;

    -- Check for duplicate email
    IF EXISTS (SELECT 1 FROM tb_customers WHERE email = v_email) THEN
        RETURN standardized_error(
            'DUPLICATE_EMAIL',
            'Customer with this email already exists',
            jsonb_build_object('field', 'email', 'existing_customer_id', (
                SELECT id FROM tb_customers WHERE email = v_email LIMIT 1
            ))
        );
    END IF;

    -- Create customer
    INSERT INTO tb_customers (
        email,
        name,
        company,
        phone
    ) VALUES (
        v_email,
        input_data->>'name',
        input_data->>'company',
        input_data->>'phone'
    ) RETURNING id INTO v_customer_id;

    -- Return standardized success
    RETURN standardized_success(
        'Customer created successfully',
        jsonb_build_object(
            'customer_id', v_customer_id,
            'email', v_email,
            'created_at', NOW()
        )
    );

EXCEPTION
    WHEN OTHERS THEN
        RETURN standardized_error(
            'INTERNAL_ERROR',
            'An unexpected error occurred: ' || SQLERRM,
            jsonb_build_object('sql_error', SQLERRM)
        );
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION fn_create_customer IS
'Creates a new customer with standardized response format for AI consumption.

Response Format:
Success: {"success": true, "message": "...", "data": {"result": {...}}}
Error: {"success": false, "message": "...", "data": {"error": {"code": "...", "message": "...", "details": {...}}}}

AI can predict and handle these consistent formats across all mutations.';
```

### Self-Describing Operations

Include operation metadata in responses:

```sql
-- View with operation metadata
CREATE OR REPLACE VIEW v_customers_with_operations AS
SELECT
    c.id,
    c.email,
    c.status,
    jsonb_build_object(
        '__typename', 'Customer',
        'id', c.id,
        'email', c.email,
        'name', c.name,
        'company', c.company,
        'status', c.status,
        'created_at', c.created_at,
        'subscription', CASE
            WHEN cs.id IS NOT NULL THEN
                jsonb_build_object(
                    'plan', sp.name,
                    'status', cs.status,
                    'next_billing_date', cs.next_billing_date
                )
            ELSE NULL
        END,
        -- Available operations based on current state
        'available_operations', jsonb_build_object(
            'can_update', true,  -- Customers can always be updated
            'can_delete', c.status = 'inactive' AND COALESCE(cs.status, 'none') != 'active',
            'can_activate', c.status = 'inactive',
            'can_deactivate', c.status = 'active',
            'can_subscribe', cs.id IS NULL,  -- No active subscription
            'can_change_plan', cs.id IS NOT NULL AND cs.status = 'active',
            'can_cancel_subscription', cs.id IS NOT NULL AND cs.status = 'active'
        ),
        -- Next possible states
        'state_transitions', CASE c.status
            WHEN 'active' THEN jsonb_build_array('inactive')
            WHEN 'inactive' THEN jsonb_build_array('active', 'deleted')
            ELSE jsonb_build_array()
        END,
        -- Related operations
        'related_operations', jsonb_build_object(
            'view_orders', '/api/customers/' || c.id || '/orders',
            'view_invoices', '/api/customers/' || c.id || '/invoices',
            'view_support_tickets', '/api/customers/' || c.id || '/tickets',
            'subscription_management', CASE
                WHEN cs.id IS NOT NULL
                THEN '/api/customers/' || c.id || '/subscription'
                ELSE NULL
            END
        )
    ) AS data
FROM tb_customers c
LEFT JOIN tb_customer_subscriptions cs ON cs.customer_id = c.id AND cs.status = 'active'
LEFT JOIN tb_subscription_plans sp ON sp.id = cs.plan_id;

COMMENT ON VIEW v_customers_with_operations IS
'Customer data with operation metadata to guide AI interactions.

AI Usage:
- Check available_operations before attempting operations
- Use state_transitions to understand valid next states
- Use related_operations to discover related endpoints
- Prevents AI from attempting invalid operations

Example AI Logic:
if customer.available_operations.can_subscribe:
    # Offer subscription creation
if customer.available_operations.can_change_plan:
    # Show plan change options
if not customer.available_operations.can_delete:
    # Explain why deletion is not available';
```

## Integration Examples

### OpenAI Function Calling

Structure functions for OpenAI's function calling API:

```python
# Generated from PostgreSQL function comments and signatures
openai_functions = [
    {
        "name": "create_customer",
        "description": "Creates a new customer with email validation and duplicate checking",
        "parameters": {
            "type": "object",
            "properties": {
                "email": {
                    "type": "string",
                    "pattern": "^[^@]+@[^@]+\\.[^@]+$",
                    "description": "Valid email address"
                },
                "name": {
                    "type": "string",
                    "description": "Customer's full name"
                },
                "company": {
                    "type": "string",
                    "description": "Company name (optional)"
                },
                "phone": {
                    "type": "string",
                    "description": "Phone number (optional)"
                }
            },
            "required": ["email", "name"]
        }
    },
    {
        "name": "change_subscription_plan",
        "description": "Changes customer's subscription plan with automatic upgrade/downgrade handling",
        "parameters": {
            "type": "object",
            "properties": {
                "customer_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Customer's unique identifier"
                },
                "to_plan_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Target subscription plan ID"
                },
                "effective_date": {
                    "type": "string",
                    "format": "date",
                    "description": "When the change takes effect (default: today)"
                }
            },
            "required": ["customer_id", "to_plan_id"]
        }
    },
    {
        "name": "get_customer_analytics",
        "description": "Retrieves comprehensive customer analytics including lifetime value and churn risk",
        "parameters": {
            "type": "object",
            "properties": {
                "customer_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Customer ID for analytics"
                },
                "include_predictions": {
                    "type": "boolean",
                    "default": false,
                    "description": "Include churn and lifetime value predictions"
                }
            },
            "required": ["customer_id"]
        }
    }
]

# AI can call these functions with natural language:
# "Create a customer named John Doe with email john@example.com"
# "Change customer abc-123 to the premium plan starting next month"
# "Show me analytics for customer xyz-789 including predictions"
```

### LangChain Integration

Create LangChain tools from FraiseQL functions:

```python
from langchain.tools import StructuredTool
from langchain.pydantic_v1 import BaseModel, Field
import asyncpg

class CreateCustomerInput(BaseModel):
    """Input for creating a new customer."""
    email: str = Field(..., description="Customer's email address")
    name: str = Field(..., description="Customer's full name")
    company: str | None = Field(None, description="Company name")
    phone: str | None = Field(None, description="Phone number")

class FraiseQLTool:
    def __init__(self, connection_url: str):
        self.connection_url = connection_url

    async def create_customer(self, input_data: CreateCustomerInput) -> dict:
        """Create a new customer with validation."""
        async with asyncpg.connect(self.connection_url) as conn:
            result = await conn.fetchrow(
                "SELECT * FROM fn_create_customer($1)",
                input_data.dict(exclude_none=True)
            )
            return dict(result)

    async def query_customers(self, query: str) -> list[dict]:
        """Query customers with natural language filters."""
        # Convert natural language to SQL WHERE clause
        sql_filter = self._natural_language_to_sql(query)

        async with asyncpg.connect(self.connection_url) as conn:
            results = await conn.fetch(
                f"SELECT data FROM v_customers_nlp_friendly WHERE {sql_filter}"
            )
            return [dict(r) for r in results]

    def _natural_language_to_sql(self, query: str) -> str:
        """Convert natural language to SQL WHERE clause."""
        # Simple mappings for common phrases
        mappings = {
            "high value customers": "(data->'metrics'->>'lifetime_value')::numeric > 1000",
            "recent customers": "created_at > NOW() - INTERVAL '30 days'",
            "churning customers": "data->'segments' ? 'churn_risk'",
            "active subscriptions": "data->'subscription'->>'status' = 'active'",
            "premium customers": "data->'subscription'->'plan'->>'name' ILIKE '%premium%'"
        }

        for phrase, sql in mappings.items():
            if phrase in query.lower():
                return sql

        # Fallback to basic search
        return f"data->'name' ILIKE '%{query}%' OR data->'company' ILIKE '%{query}%'"

# Create LangChain tools
fraiseql_tool = FraiseQLTool("postgresql://...")

create_customer_tool = StructuredTool.from_function(
    func=fraiseql_tool.create_customer,
    name="create_customer",
    description="Create a new customer with email validation and duplicate checking",
    args_schema=CreateCustomerInput
)

# AI Agent can now use these tools naturally:
# "Create a customer for Jane Smith at jane@acme.com who works at Acme Corp"
# "Show me all high value customers from the last month"
```

### LlamaIndex Integration

Create LlamaIndex QueryEngine with FraiseQL schema knowledge:

```python
from llama_index import SimpleDirectoryReader, VectorStoreIndex
from llama_index.llms import OpenAI
from llama_index.tools import QueryEngineTool
import asyncpg

class FraiseQLQueryEngine:
    """Query engine that understands FraiseQL schema and business context."""

    def __init__(self, connection_url: str):
        self.connection_url = connection_url
        self.schema_docs = self._load_schema_documentation()
        self.index = VectorStoreIndex.from_documents(self.schema_docs)

    def _load_schema_documentation(self):
        """Load schema documentation from database comments."""
        # This would extract COMMENT ON statements and create documents
        # containing table/view/function documentation for the index
        pass

    async def query(self, query_str: str) -> str:
        """Answer questions about data using schema knowledge."""

        # First, understand what the user is asking about
        query_engine = self.index.as_query_engine()
        schema_context = query_engine.query(
            f"What database tables and views would I need to answer: {query_str}"
        )

        # Generate and execute SQL query
        sql_query = await self._generate_sql_query(query_str, schema_context)

        async with asyncpg.connect(self.connection_url) as conn:
            results = await conn.fetch(sql_query)
            return self._format_results(results, query_str)

    async def _generate_sql_query(self, question: str, schema_context: str) -> str:
        """Generate SQL query based on question and schema context."""
        llm = OpenAI(model="gpt-4")

        prompt = f"""
        Based on this schema context:
        {schema_context}

        Generate a PostgreSQL query to answer: {question}

        Use these FraiseQL conventions:
        - Query views (v_*) not tables (tb_*)
        - Extract data from JSONB 'data' column
        - Use proper JSONB operators (->>, ->, ?)
        - Include relevant WHERE clauses for performance

        Return only the SQL query:
        """

        response = await llm.aquery(prompt)
        return response.text.strip()

    def _format_results(self, results: list, original_question: str) -> str:
        """Format query results into natural language response."""
        if not results:
            return f"I couldn't find any data to answer: {original_question}"

        # Convert results to natural language based on question type
        if "how many" in original_question.lower():
            return f"I found {len(results)} results."
        elif "who are" in original_question.lower():
            names = [r['data'].get('name', 'Unknown') for r in results]
            return f"Here are the results: {', '.join(names[:10])}"
        else:
            # Generic response with first few results
            summary = []
            for r in results[:3]:
                data = r.get('data', {})
                summary.append(str(data))
            return f"Here are the top results:\n" + '\n'.join(summary)

# Usage
engine = FraiseQLQueryEngine("postgresql://...")

# AI can answer business questions naturally:
# "How many customers signed up last month?"
# "Who are our highest value customers?"
# "Which products are running low on inventory?"
# "Show me customers at risk of churning"
```

## AI Development Workflows

### Automated API Generation

Generate client SDKs from database schema:

```python
# Schema introspection for API generation
async def generate_api_client(connection_url: str) -> str:
    """Generate TypeScript client from FraiseQL schema."""

    async with asyncpg.connect(connection_url) as conn:
        # Get all views and their structures
        views = await conn.fetch("""
            SELECT
                table_name,
                obj_description(c.oid) as comment
            FROM information_schema.tables t
            JOIN pg_class c ON c.relname = t.table_name
            WHERE table_schema = 'public'
                AND table_name LIKE 'v_%'
                AND table_type = 'VIEW'
        """)

        # Get function signatures for mutations
        functions = await conn.fetch("""
            SELECT
                routine_name,
                parameters,
                obj_description(p.oid) as comment
            FROM information_schema.routines r
            JOIN pg_proc p ON p.proname = r.routine_name
            WHERE routine_schema = 'public'
                AND routine_name LIKE 'fn_%'
        """)

        # Generate TypeScript interfaces
        client_code = generate_typescript_client(views, functions)

        return client_code

def generate_typescript_client(views: list, functions: list) -> str:
    """Generate TypeScript client code."""

    interfaces = []

    # Generate interfaces from view schemas
    for view in views:
        interface = f"""
// {view['comment']}
interface {to_pascal_case(view['table_name'])} {{
    id: string;
    data: {{
        // Auto-generated from view structure
        __typename: string;
        [key: string]: any;
    }};
}}
"""
        interfaces.append(interface)

    # Generate mutation functions
    mutations = []
    for func in functions:
        mutation = f"""
// {func['comment']}
async {to_camel_case(func['routine_name'])}(
    input: {to_pascal_case(func['routine_name'])}Input
): Promise<MutationResult> {{
    return this.executeMutation('{func['routine_name']}', input);
}}
"""
        mutations.append(mutation)

    return f"""
// Auto-generated FraiseQL client
// Generated from database schema on {datetime.now()}

{chr(10).join(interfaces)}

class FraiseQLClient {{
    constructor(private apiUrl: string) {{}}

    {chr(10).join(mutations)}

    private async executeMutation(
        functionName: string,
        input: any
    ): Promise<MutationResult> {{
        // Implementation
    }}
}}
"""
```

### Schema-Aware Code Generation

Generate resolvers from database functions:

```python
# Auto-generate GraphQL resolvers from database schema
def generate_graphql_resolvers(connection_url: str) -> str:
    """Generate GraphQL resolvers from FraiseQL functions."""

    resolver_code = """
# Auto-generated GraphQL resolvers
from fraiseql import query, mutation
# Modern typing patterns used (built-in types)

"""

    # Generate query resolvers from views
    views = get_views_from_db(connection_url)
    for view in views:
        resolver_code += f"""
@query
async def {to_snake_case(view['name'])}(
    where: dict | None = None,
    order_by: str | None = None,
    limit: int | None = 20,
    context = None
) -> list[{to_pascal_case(view['name'])}]:
    '''
    {view['comment']}

    Auto-generated resolver for {view['name']}
    '''
    # FraiseQL handles the query automatically
    pass
"""

    # Generate mutation resolvers from functions
    functions = get_functions_from_db(connection_url)
    for func in functions:
        resolver_code += f"""
@mutation
async def {to_snake_case(func['name'])}(
    input: {to_pascal_case(func['name'])}Input,
    context = None
) -> {to_pascal_case(func['name'])}Success | {to_pascal_case(func['name'])}Error:
    '''
    {func['comment']}

    Auto-generated mutation for {func['name']}
    '''
    # FraiseQL handles the function call automatically
    pass
"""

    return resolver_code
```

## Performance Implications

### AI Query Optimization

Optimize views for AI query patterns:

```sql
-- AI-optimized view with strategic indexing
CREATE TABLE tv_ai_customer_summary AS
SELECT
    c.id,
    c.email,
    c.name,
    c.company,
    c.created_at,
    -- Pre-computed metrics that AI often queries
    COALESCE(metrics.order_count, 0) as order_count,
    COALESCE(metrics.lifetime_value, 0) as lifetime_value,
    COALESCE(metrics.avg_order_value, 0) as avg_order_value,
    COALESCE(metrics.days_since_last_order, 999) as days_since_last_order,
    -- Pre-computed segments for fast filtering
    CASE
        WHEN COALESCE(metrics.lifetime_value, 0) > 5000 THEN 'vip'
        WHEN COALESCE(metrics.lifetime_value, 0) > 1000 THEN 'high_value'
        WHEN COALESCE(metrics.order_count, 0) > 5 THEN 'loyal'
        WHEN COALESCE(metrics.days_since_last_order, 999) > 90 THEN 'at_risk'
        ELSE 'standard'
    END as segment,
    -- Full JSON for API response
    jsonb_build_object(
        '__typename', 'CustomerSummary',
        'id', c.id,
        'email', c.email,
        'name', c.name,
        'company', c.company,
        'segment', segment,
        'metrics', jsonb_build_object(
            'order_count', COALESCE(metrics.order_count, 0),
            'lifetime_value', COALESCE(metrics.lifetime_value, 0),
            'avg_order_value', COALESCE(metrics.avg_order_value, 0),
            'days_since_last_order', COALESCE(metrics.days_since_last_order, 999)
        ),
        'created_at', c.created_at
    ) AS data,
    -- Search vector for text queries
    to_tsvector('english',
        COALESCE(c.name, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        c.email
    ) AS search_vector
FROM tb_customers c
LEFT JOIN (
    SELECT
        customer_id,
        COUNT(*) as order_count,
        SUM(total_amount) as lifetime_value,
        AVG(total_amount) as avg_order_value,
        EXTRACT(days FROM NOW() - MAX(created_at))::int as days_since_last_order
    FROM tb_orders
    WHERE status = 'completed'
    GROUP BY customer_id
) metrics ON metrics.customer_id = c.id;

-- AI-friendly indexes
CREATE INDEX idx_ai_customer_segment ON tv_ai_customer_summary(segment);
CREATE INDEX idx_ai_customer_value ON tv_ai_customer_summary(lifetime_value);
CREATE INDEX idx_ai_customer_risk ON tv_ai_customer_summary(days_since_last_order);
CREATE INDEX idx_ai_customer_search ON tv_ai_customer_summary USING gin(search_vector);
CREATE INDEX idx_ai_customer_created ON tv_ai_customer_summary(created_at);

-- View that uses the optimized table
CREATE OR REPLACE VIEW v_customers_ai_optimized AS
SELECT
    id,
    email,
    segment,  -- Exposed for filtering
    lifetime_value,  -- Exposed for range queries
    days_since_last_order,  -- Exposed for churn analysis
    data,
    search_vector  -- Exposed for text search
FROM tv_ai_customer_summary;

COMMENT ON VIEW v_customers_ai_optimized IS
'AI-optimized customer view with pre-computed segments and fast filtering.

Optimized AI Query Patterns:
- Segment filtering: WHERE segment = ''vip''
- Value range: WHERE lifetime_value BETWEEN 1000 AND 5000
- Churn analysis: WHERE days_since_last_order > 90
- Text search: WHERE search_vector @@ plainto_tsquery(''acme corp'')
- Date ranges: WHERE created_at > NOW() - INTERVAL ''1 year''

All common AI queries hit indexes and return sub-millisecond.';
```

## Best Practices for AI Integration

### 1. Design for Discoverability

- Use consistent naming conventions across all objects
- Include comprehensive comments on all schema objects
- Provide example queries in view comments
- Use semantic field names that match business terminology

### 2. Enable Safe Exploration

- Use strong typing and constraints to prevent errors
- Provide clear error messages with structured codes
- Include operation metadata in responses
- Use read-only views for AI querying

### 3. Optimize for Common Patterns

- Pre-compute metrics that AI frequently needs
- Create specialized views for AI use cases
- Index fields commonly used in AI-generated queries
- Batch operations that AI might call repeatedly

### 4. Provide Rich Context

- Document business rules in function comments
- Include example use cases and query patterns
- Explain the meaning of calculated fields
- Link related operations and endpoints

### 5. Support Natural Language

- Design field names that match common business terms
- Create views that answer common business questions
- Use enums and constants that reflect real-world concepts
- Structure data to support intuitive filtering

## Future Considerations

### Autonomous Schema Evolution

- AI systems could suggest schema improvements based on query patterns
- Automated index creation based on AI query analysis
- Self-optimizing views that adapt to AI usage patterns
- Intelligent materialization of frequently accessed AI computations

### Advanced AI Integrations

- Real-time model inference in PostgreSQL functions
- Vector similarity search integrated with business data
- AI-driven data quality monitoring and correction
- Automated business rule extraction from data patterns

## Next Steps

- Review [Domain-Driven Database Design](./domain-driven-database.md) for foundational patterns
- Explore [Database API Design Patterns](./database-api-patterns.md) for advanced techniques
- See practical implementation in the [Blog API Tutorial](../tutorials/blog-api.md)
