-- Orders Service Tables (Trinity Pattern)
-- Pattern: tb_* (table), pk_* (BIGINT primary key), id (UUID natural key), v_* (view)

CREATE TABLE tb_orders (
    pk_order BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    total DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

CREATE TABLE tb_order_items (
    pk_order_item BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    fk_order BIGINT NOT NULL,
    product_id VARCHAR(36) NOT NULL,
    quantity INT NOT NULL,
    price DECIMAL(10, 2) NOT NULL,
    FOREIGN KEY (fk_order) REFERENCES tb_orders(pk_order),
    INDEX idx_tb_fk_order (fk_order)
);

CREATE INDEX idx_tb_orders_id ON tb_orders(id);
CREATE INDEX idx_tb_orders_user_id ON tb_orders(user_id);
CREATE INDEX idx_tb_orders_status ON tb_orders(status);
CREATE INDEX idx_tb_order_items_id ON tb_order_items(id);

-- Create views (Trinity Pattern v_* naming)
CREATE VIEW v_orders AS
SELECT pk_order, id, user_id, status, total, created_at, updated_at
FROM tb_orders;

CREATE VIEW v_order_items AS
SELECT pk_order_item, id, fk_order, product_id, quantity, price
FROM tb_order_items;

-- Inventory Service Tables (in fraiseql_inventory database, Trinity Pattern)
USE fraiseql_inventory;

CREATE TABLE tb_products (
    pk_product BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    stock INT NOT NULL,
    price DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

CREATE TABLE tb_reservations (
    pk_reservation BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    order_id VARCHAR(36) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'reserved',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

CREATE TABLE tb_reservation_items (
    pk_reservation_item BIGINT AUTO_INCREMENT PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    fk_reservation BIGINT NOT NULL,
    product_id VARCHAR(36) NOT NULL,
    quantity INT NOT NULL,
    FOREIGN KEY (fk_reservation) REFERENCES tb_reservations(pk_reservation),
    INDEX idx_tb_fk_reservation (fk_reservation)
);

CREATE INDEX idx_tb_products_id ON tb_products(id);
CREATE INDEX idx_tb_reservations_id ON tb_reservations(id);
CREATE INDEX idx_tb_reservations_order_id ON tb_reservations(order_id);
CREATE INDEX idx_tb_reservations_status ON tb_reservations(status);
CREATE INDEX idx_tb_reservation_items_id ON tb_reservation_items(id);

-- Create views (Trinity Pattern v_* naming)
CREATE VIEW v_products AS
SELECT pk_product, id, name, stock, price, created_at, updated_at
FROM tb_products;

CREATE VIEW v_reservations AS
SELECT pk_reservation, id, order_id, status, created_at, updated_at
FROM tb_reservations;

CREATE VIEW v_reservation_items AS
SELECT pk_reservation_item, id, fk_reservation, product_id, quantity
FROM tb_reservation_items;

-- Sample inventory
INSERT INTO tb_products (id, name, stock, price) VALUES
  ('prod-001', 'Laptop', 50, 999.99),
  ('prod-002', 'Mouse', 200, 29.99),
  ('prod-003', 'Keyboard', 150, 79.99);

USE fraiseql;
