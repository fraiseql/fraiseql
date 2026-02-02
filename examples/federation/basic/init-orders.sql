-- Orders database initialization (Trinity Pattern)
-- Used by orders-service (owns Order entity, extends User)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (federation ID), v_* (view)

DROP TABLE IF EXISTS tb_order CASCADE;

CREATE TABLE tb_order (
    pk_order SERIAL PRIMARY KEY,
    id VARCHAR(50) UNIQUE NOT NULL,
    user_id VARCHAR(50) NOT NULL,
    status VARCHAR(50) DEFAULT 'pending',
    total DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Index key fields for federation and performance
CREATE INDEX idx_tb_order_id ON tb_order(id);
CREATE INDEX idx_tb_order_user_id ON tb_order(user_id);
CREATE INDEX idx_tb_order_status ON tb_order(status);

-- Create view (Trinity Pattern v_* naming)
-- Returns pk_* (for internal joins) and data (JSONB for GraphQL)
CREATE VIEW v_order AS
SELECT
    pk_order,
    jsonb_build_object(
        'id', id,
        'userId', user_id,
        'status', status,
        'total', total,
        'createdAt', created_at
    ) AS data
FROM tb_order;

-- Insert test data
INSERT INTO tb_order (id, user_id, status, total) VALUES
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
