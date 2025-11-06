-- Product Catalog LTREE Example
-- Demonstrates product categorization hierarchies

CREATE EXTENSION IF NOT EXISTS ltree;

-- Product catalog table
CREATE TABLE tb_product (
    pk_product INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL,
    description TEXT,
    price DECIMAL(10,2),
    category_path LTREE NOT NULL,  -- Hierarchical category path
    sku TEXT UNIQUE,
    in_stock BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

-- GiST index for category queries
CREATE INDEX idx_tb_product_category_path ON tb_product USING GIST (category_path);

-- Additional indexes for common queries
CREATE INDEX idx_tb_product_price ON tb_product (price);
CREATE INDEX idx_tb_product_in_stock ON tb_product (in_stock);
CREATE INDEX idx_tb_product_category_depth ON tb_product (nlevel(category_path));
CREATE INDEX idx_tb_product_id ON tb_product (id);

-- Sample product catalog
INSERT INTO tb_product (name, description, price, category_path, sku) VALUES
-- Electronics
('MacBook Pro 16"', 'High-performance laptop for professionals', 2499.99, 'electronics.computers.laptops.apple', 'MBP16-2024'),
('Dell XPS 13', 'Ultra-portable laptop with premium build', 1299.99, 'electronics.computers.laptops.dell', 'DXPS13-2024'),
('iPad Pro 12.9"', 'Professional tablet with Apple Pencil support', 1099.99, 'electronics.tablets.apple.ipad', 'IPADPRO-129'),
('Samsung Galaxy Tab S8', 'Android tablet with S Pen', 699.99, 'electronics.tablets.samsung.galaxy', 'TAB-S8'),
('Sony WH-1000XM4', 'Noise-canceling wireless headphones', 349.99, 'electronics.audio.headphones.sony', 'WH1000XM4'),
('Bose QuietComfort 35', 'Premium noise-canceling headphones', 299.99, 'electronics.audio.headphones.bose', 'QC35-II'),

-- Home & Garden
('Dyson V15 Detect', 'Cordless vacuum with laser dust detection', 749.99, 'home.kitchen.appliances.vacuums.dyson', 'V15-DETECT'),
('KitchenAid Stand Mixer', 'Professional stand mixer for baking', 379.99, 'home.kitchen.appliances.mixers.kitchenaid', 'KSM150'),
('Philips Air Fryer', 'Healthy frying with little to no oil', 129.99, 'home.kitchen.appliances.air_fryers.philips', 'AIRFRYER-XL'),
('Weber Genesis II', 'Gas grill with side burner', 1099.99, 'home.outdoor.grills.gas.weber', 'GENESIS-II'),

-- Sports & Outdoors
('Nike Air Zoom Pegasus', 'Versatile running shoes', 129.99, 'sports.footwear.running.nike', 'AIR-ZOOM-PEGASUS'),
('Adidas Ultraboost 22', 'Comfortable running shoes with Boost technology', 189.99, 'sports.footwear.running.adidas', 'ULTRABOOST-22'),
('Patagonia Better Sweater', 'Fleece sweater made from recycled materials', 139.99, 'sports.clothing.fleece.patagonia', 'BETTER-SWEATER'),
('REI Co-op Flash 55', 'Lightweight backpacking backpack', 169.99, 'sports.gear.backpacks.rei', 'FLASH-55'),

-- Books
('Clean Code', 'A Handbook of Agile Software Craftsmanship', 39.99, 'books.technical.programming.clean_code', 'CC-ROBERT-MARTIN'),
('The Pragmatic Programmer', 'Your journey to mastery', 49.99, 'books.technical.programming.pragmatic', 'PP-HUNT-THOMAS'),
('Designing Data-Intensive Applications', 'The Big Ideas Behind Reliable, Scalable Systems', 59.99, 'books.technical.systems_design.ddia', 'DDIA-KLEPPMANN'),
('Atomic Habits', 'An Easy & Proven Way to Build Good Habits', 16.99, 'books.self_help.habits.atomic', 'AH-JAMES-CLEAR'),

-- Toys & Games
('LEGO Creator 3-in-1', 'Deep Sea Creatures building set', 89.99, 'toys.building.lego.creator', 'LEGO-31088'),
('Codenames', 'Word-based team game', 19.99, 'games.board.social_deduction.codenames', 'CODENAMES-VLAADA'),
('Ticket to Ride', 'Cross-country train adventure', 49.99, 'games.board.euro.trainticket', 'TTR-2019'),

-- Health & Beauty
('Dyson Airwrap', 'Multi-styling tool for hair', 599.99, 'beauty.hair.styling.dyson', 'AIRWRAP-COMPLETE'),
('Foreo Luna 3', 'Facial cleansing device', 279.99, 'beauty.skincare.cleansing.foreo', 'LUNA3'),
('Peloton Bike', 'Connected fitness bike with classes', 2499.99, 'fitness.equipment.bikes.peloton', 'PELOTON-BIKE'),
('Theragun Pro', 'Percussion massage device', 599.99, 'fitness.recovery.massagers.theragun', 'THERAGUN-PRO');

-- Analyze for query optimization
ANALYZE tb_product;
