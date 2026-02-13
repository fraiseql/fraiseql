-- SQL Server Analytics Test Data
--
-- This script creates fact tables for testing analytics introspection and aggregation.

USE fraiseql_test;
GO

-- ============================================================================
-- Fact Table: tf_sales (sales transactions)
-- ============================================================================

IF OBJECT_ID('dbo.tf_sales', 'U') IS NOT NULL DROP TABLE dbo.tf_sales;
GO

CREATE TABLE dbo.tf_sales (
    id BIGINT IDENTITY(1,1) PRIMARY KEY,

    -- Measures (numeric columns for aggregation)
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,
    discount DECIMAL(10,2) DEFAULT 0.00,

    -- Dimensions (NVARCHAR(MAX) JSON for flexible grouping)
    data NVARCHAR(MAX) NOT NULL,

    -- Denormalized filters (indexed for fast WHERE)
    customer_id INT NOT NULL,
    product_id INT NOT NULL,
    occurred_at DATETIME2 NOT NULL,
    created_at DATETIME2 DEFAULT GETUTCDATE()
);
GO

-- Create indexes for denormalized filters
CREATE INDEX idx_sales_customer ON dbo.tf_sales(customer_id);
CREATE INDEX idx_sales_product ON dbo.tf_sales(product_id);
CREATE INDEX idx_sales_occurred ON dbo.tf_sales(occurred_at);
GO

-- Insert test data
INSERT INTO dbo.tf_sales (revenue, quantity, cost, discount, data, customer_id, product_id, occurred_at) VALUES
    -- Electronics sales
    (999.99, 1, 700.00, 0.00, N'{"category": "electronics", "region": "US", "channel": "online"}',
     1, 1, '2024-01-15 10:30:00'),
    (29.99, 2, 15.00, 5.00, N'{"category": "electronics", "region": "UK", "channel": "online"}',
     2, 2, '2024-01-16 14:20:00'),
    (999.99, 1, 700.00, 100.00, N'{"category": "electronics", "region": "FR", "channel": "store"}',
     1, 1, '2024-01-17 09:15:00'),

    -- Furniture sales
    (299.99, 1, 180.00, 0.00, N'{"category": "furniture", "region": "US", "channel": "store"}',
     3, 3, '2024-01-18 11:45:00'),
    (199.99, 2, 120.00, 20.00, N'{"category": "furniture", "region": "DE", "channel": "online"}',
     4, 4, '2024-01-19 16:30:00'),
    (299.99, 1, 180.00, 30.00, N'{"category": "furniture", "region": "JP", "channel": "online"}',
     5, 3, '2024-01-20 08:00:00'),

    -- More electronics
    (29.99, 5, 15.00, 0.00, N'{"category": "electronics", "region": "US", "channel": "online"}',
     2, 2, '2024-01-21 13:25:00'),
    (999.99, 1, 700.00, 50.00, N'{"category": "electronics", "region": "UK", "channel": "store"}',
     3, 1, '2024-01-22 10:10:00');
GO

-- ============================================================================
-- Fact Table: tf_events (event logs)
-- ============================================================================

IF OBJECT_ID('dbo.tf_events', 'U') IS NOT NULL DROP TABLE dbo.tf_events;
GO

CREATE TABLE dbo.tf_events (
    id BIGINT IDENTITY(1,1) PRIMARY KEY,

    -- Measures
    duration_ms BIGINT NOT NULL,
    error_count INT DEFAULT 0,
    request_size BIGINT DEFAULT 0,
    response_size BIGINT DEFAULT 0,

    -- Dimensions
    data NVARCHAR(MAX) NOT NULL,

    -- Denormalized filters
    user_id INT,
    endpoint VARCHAR(255) NOT NULL,
    status_code INT NOT NULL,
    occurred_at DATETIME2 NOT NULL,
    created_at DATETIME2 DEFAULT GETUTCDATE()
);
GO

-- Create indexes
CREATE INDEX idx_events_user ON dbo.tf_events(user_id);
CREATE INDEX idx_events_endpoint ON dbo.tf_events(endpoint);
CREATE INDEX idx_events_status ON dbo.tf_events(status_code);
CREATE INDEX idx_events_occurred ON dbo.tf_events(occurred_at);
GO

-- Insert test data
INSERT INTO dbo.tf_events (duration_ms, error_count, request_size, response_size, data, user_id, endpoint, status_code, occurred_at) VALUES
    (150, 0, 512, 2048, N'{"method": "GET", "version": "v1", "client": "web"}',
     1, '/api/users', 200, '2024-01-15 10:00:00'),
    (250, 0, 1024, 4096, N'{"method": "POST", "version": "v1", "client": "mobile"}',
     2, '/api/users', 201, '2024-01-15 10:05:00'),
    (50, 1, 256, 128, N'{"method": "GET", "version": "v1", "client": "web"}',
     3, '/api/posts', 404, '2024-01-15 10:10:00'),
    (180, 0, 768, 3072, N'{"method": "GET", "version": "v2", "client": "web"}',
     1, '/api/posts', 200, '2024-01-15 10:15:00'),
    (5000, 1, 512, 256, N'{"method": "POST", "version": "v1", "client": "mobile"}',
     4, '/api/orders', 500, '2024-01-15 10:20:00');
GO

-- ============================================================================
-- Non-Fact Table: ta_sales_by_day (aggregate table - for testing rejection)
-- ============================================================================

IF OBJECT_ID('dbo.ta_sales_by_day', 'U') IS NOT NULL DROP TABLE dbo.ta_sales_by_day;
GO

CREATE TABLE dbo.ta_sales_by_day (
    id BIGINT IDENTITY(1,1) PRIMARY KEY,
    day DATE NOT NULL UNIQUE,
    total_revenue DECIMAL(10,2) NOT NULL,
    total_quantity INT NOT NULL,
    transaction_count INT NOT NULL,
    data NVARCHAR(MAX) NOT NULL,
    created_at DATETIME2 DEFAULT GETUTCDATE()
);
GO
