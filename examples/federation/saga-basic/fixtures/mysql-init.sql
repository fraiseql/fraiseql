-- Orders Service Tables (Trinity Pattern)
-- Pattern: tb_* (table), pk_* (BIGINT primary key), id (UUID natural key), v_* (view)

CREATE TABLE tb_order (
    pk_order BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    total DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

CREATE TABLE tb_order_item (
    pk_order_item BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    fk_order BIGINT NOT NULL,
    product_id VARCHAR(36) NOT NULL,
    quantity INT NOT NULL,
    price DECIMAL(10, 2) NOT NULL,
    FOREIGN KEY (fk_order) REFERENCES tb_order(pk_order),
    INDEX idx_tb_fk_order (fk_order)
);

CREATE INDEX idx_tb_order_id ON tb_order(id);
CREATE INDEX idx_tb_order_user_id ON tb_order(user_id);
CREATE INDEX idx_tb_order_status ON tb_order(status);
CREATE INDEX idx_tb_order_item_id ON tb_order_item(id);

-- Create views (Trinity Pattern v_* naming)
-- Returns pk_* (for internal joins) and data (JSON for GraphQL)
CREATE VIEW v_order AS
SELECT
    pk_order,
    JSON_OBJECT(
        'id', id,
        'userId', user_id,
        'status', status,
        'total', total,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) AS data
FROM tb_order;

CREATE VIEW v_order_item AS
SELECT
    pk_order_item,
    JSON_OBJECT(
        'id', id,
        'productId', product_id,
        'quantity', quantity,
        'price', price
    ) AS data
FROM tb_order_item;

-- Inventory Service Tables (in fraiseql_inventory database, Trinity Pattern)
USE fraiseql_inventory;

CREATE TABLE tb_product (
    pk_product BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    stock INT NOT NULL,
    price DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

CREATE TABLE tb_reservation (
    pk_reservation BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    order_id VARCHAR(36) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'reserved',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

CREATE TABLE tb_reservation_item (
    pk_reservation_item BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    fk_reservation BIGINT NOT NULL,
    product_id VARCHAR(36) NOT NULL,
    quantity INT NOT NULL,
    FOREIGN KEY (fk_reservation) REFERENCES tb_reservation(pk_reservation),
    INDEX idx_tb_fk_reservation (fk_reservation)
);

CREATE INDEX idx_tb_product_id ON tb_product(id);
CREATE INDEX idx_tb_reservation_id ON tb_reservation(id);
CREATE INDEX idx_tb_reservation_order_id ON tb_reservation(order_id);
CREATE INDEX idx_tb_reservation_status ON tb_reservation(status);
CREATE INDEX idx_tb_reservation_item_id ON tb_reservation_item(id);

-- Create views (Trinity Pattern v_* naming)
-- Returns pk_* (for internal joins) and data (JSON for GraphQL)
CREATE VIEW v_product AS
SELECT
    pk_product,
    JSON_OBJECT(
        'id', id,
        'name', name,
        'stock', stock,
        'price', price,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) AS data
FROM tb_product;

CREATE VIEW v_reservation AS
SELECT
    pk_reservation,
    JSON_OBJECT(
        'id', id,
        'orderId', order_id,
        'status', status,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) AS data
FROM tb_reservation;

CREATE VIEW v_reservation_item AS
SELECT
    pk_reservation_item,
    JSON_OBJECT(
        'id', id,
        'productId', product_id,
        'quantity', quantity
    ) AS data
FROM tb_reservation_item;

-- Sample inventory
INSERT INTO tb_product (id, name, stock, price) VALUES
  ('prod-001', 'Laptop', 50, 999.99),
  ('prod-002', 'Mouse', 200, 29.99),
  ('prod-003', 'Keyboard', 150, 79.99);

USE fraiseql;
