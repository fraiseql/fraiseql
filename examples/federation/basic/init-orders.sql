-- Orders database initialization
-- Used by orders-service (owns Order entity, extends User)

CREATE TABLE orders (
    id VARCHAR(50) PRIMARY KEY,
    user_id VARCHAR(50) NOT NULL,
    status VARCHAR(50) DEFAULT 'pending',
    total DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Index key fields for federation
CREATE INDEX idx_orders_id ON orders(id);
CREATE INDEX idx_orders_user_id ON orders(user_id);

-- Insert test data
INSERT INTO orders (id, user_id, status, total) VALUES
    ('order1', 'user1', 'completed', 99.99),
    ('order2', 'user1', 'completed', 149.99),
    ('order3', 'user1', 'pending', 199.99),
    ('order4', 'user2', 'completed', 249.99),
    ('order5', 'user2', 'pending', 299.99),
    ('order6', 'user3', 'completed', 59.99),
    ('order7', 'user3', 'pending', 79.99),
    ('order8', 'user4', 'completed', 349.99),
    ('order9', 'user5', 'completed', 89.99),
    ('order10', 'user5', 'pending', 129.99);
