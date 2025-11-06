-- Auto-Documentation Example Database Schema
-- E-commerce product catalog with reviews

-- Products table
CREATE TABLE tb_product (
    pk_product INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    data JSONB NOT NULL,
    category VARCHAR(50) NOT NULL,
    in_stock BOOLEAN NOT NULL DEFAULT true,
    price DECIMAL(10,2) NOT NULL,
    average_rating DECIMAL(3,2),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_tb_product_category ON tb_product(category);
CREATE INDEX idx_tb_product_in_stock ON tb_product(in_stock) WHERE in_stock = true;
CREATE INDEX idx_tb_product_price ON tb_product(price);
CREATE INDEX idx_tb_product_rating ON tb_product(average_rating) WHERE average_rating IS NOT NULL;
CREATE INDEX idx_tb_product_id ON tb_product(id);

-- Products view (optimized for GraphQL queries)
CREATE VIEW v_product AS
SELECT
    pk_product,
    id,
    jsonb_build_object(
        'id', id,
        'name', data->>'name',
        'description', data->>'description',
        'price', price,
        'category', category,
        'in_stock', in_stock,
        'stock_quantity', (data->>'stock_quantity')::int,
        'average_rating', average_rating,
        'review_count', (data->>'review_count')::int,
        'created_at', created_at,
        'updated_at', updated_at
    ) as data
FROM tb_product;

-- Reviews table
CREATE TABLE tb_review (
    pk_review INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_product INT NOT NULL REFERENCES tb_product(pk_product) ON DELETE CASCADE,
    data JSONB NOT NULL,
    rating INT NOT NULL CHECK (rating >= 1 AND rating <= 5),
    verified_purchase BOOLEAN NOT NULL DEFAULT false,
    helpful_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes for reviews
CREATE INDEX idx_tb_review_fk_product ON tb_review(fk_product);
CREATE INDEX idx_tb_review_id ON tb_review(id);
CREATE INDEX idx_tb_review_rating ON tb_review(rating);
CREATE INDEX idx_tb_review_verified ON tb_review(verified_purchase) WHERE verified_purchase = true;
CREATE INDEX idx_tb_review_created ON tb_review(created_at DESC);

-- Reviews view
CREATE VIEW v_review AS
SELECT
    pk_review,
    id,
    jsonb_build_object(
        'id', id,
        'fk_product', fk_product,
        'customer_name', data->>'customer_name',
        'rating', rating,
        'title', data->>'title',
        'content', data->>'content',
        'verified_purchase', verified_purchase,
        'helpful_count', helpful_count,
        'created_at', created_at
    ) as data
FROM tb_review;

-- Customers table
CREATE TABLE tb_customer (
    pk_customer INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    membership_tier VARCHAR(20) NOT NULL DEFAULT 'basic',
    total_orders INT NOT NULL DEFAULT 0,
    total_spent DECIMAL(10,2) NOT NULL DEFAULT 0,
    account_created TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Sample data
INSERT INTO tb_product (data, category, in_stock, price, average_rating) VALUES
(
    '{"name": "Wireless Headphones", "description": "Premium wireless over-ear headphones with active noise cancellation.", "stock_quantity": 50, "review_count": 127}',
    'electronics',
    true,
    149.99,
    4.5
),
(
    '{"name": "Running Shoes", "description": "Lightweight running shoes with advanced cushioning technology.", "stock_quantity": 30, "review_count": 89}',
    'sports',
    true,
    89.99,
    4.7
),
(
    '{"name": "Python Programming Book", "description": "Comprehensive guide to Python programming for beginners and experts.", "stock_quantity": 100, "review_count": 234}',
    'books',
    true,
    39.99,
    4.8
),
(
    '{"name": "Smart Watch", "description": "Fitness tracking smartwatch with heart rate monitor and GPS.", "stock_quantity": 0, "review_count": 56}',
    'electronics',
    false,
    199.99,
    4.2
);

-- Sample reviews
INSERT INTO tb_review (fk_product, data, rating, verified_purchase, helpful_count) VALUES
(
    1,
    '{"customer_name": "John D.", "title": "Amazing sound quality!", "content": "These headphones exceed my expectations. The noise cancellation is superb and battery life is excellent."}',
    5,
    true,
    42
),
(
    1,
    '{"customer_name": "Sarah M.", "title": "Good but pricey", "content": "Great headphones but a bit expensive. Sound quality is excellent though."}',
    4,
    true,
    18
),
(
    2,
    '{"customer_name": "Mike R.", "title": "Perfect for running", "content": "Very comfortable and supportive. I have run over 100 miles in these shoes."}',
    5,
    true,
    35
),
(
    3,
    '{"customer_name": "Emily L.", "title": "Best Python resource", "content": "This book helped me go from beginner to confident Python developer. Highly recommended!"}',
    5,
    true,
    87
);

-- Sample customers
INSERT INTO tb_customer (email, name, membership_tier, total_orders, total_spent) VALUES
('john.doe@example.com', 'John Doe', 'premium', 15, 1249.85),
('sarah.miller@example.com', 'Sarah Miller', 'vip', 42, 3899.50),
('mike.roberts@example.com', 'Mike Roberts', 'basic', 3, 189.97);
