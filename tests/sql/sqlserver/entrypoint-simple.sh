#!/bin/sh
# SQL Server initialization entrypoint script
# Runs SQL Server and initializes test database with embedded SQL

SA_PASSWORD="${MSSQL_SA_PASSWORD}"
SQLCMD="/opt/mssql-tools18/bin/sqlcmd"

# Start SQL Server in background
/opt/mssql/bin/sqlservr &
SQLSERVER_PID=$!

echo "Waiting for SQL Server to be ready..."
for i in $(seq 1 60); do
  if $SQLCMD -S localhost -U sa -P "$SA_PASSWORD" -C -Q "SELECT 1" >/dev/null 2>&1; then
    echo "SQL Server is ready!"
    break
  fi
  echo "Attempt $i: SQL Server not ready yet, waiting..."
  sleep 1
done

echo "Creating test database and schema..."

# Create database and tables with embedded SQL
$SQLCMD -S localhost -U sa -P "$SA_PASSWORD" -C << 'EOF'
CREATE DATABASE fraiseql_test;
GO

USE fraiseql_test;
GO

-- Create users table
CREATE TABLE dbo.users (
    id INT IDENTITY(1,1) PRIMARY KEY,
    name NVARCHAR(255) NOT NULL,
    email NVARCHAR(255) NOT NULL UNIQUE,
    created_at DATETIME2 DEFAULT GETUTCDATE()
);
GO

-- Create posts table
CREATE TABLE dbo.posts (
    id INT IDENTITY(1,1) PRIMARY KEY,
    title NVARCHAR(255) NOT NULL,
    content NVARCHAR(MAX),
    author_id INT NOT NULL REFERENCES dbo.users(id),
    published BIT DEFAULT 0,
    created_at DATETIME2 DEFAULT GETUTCDATE()
);
GO

-- Create products table
CREATE TABLE dbo.products (
    id INT IDENTITY(1,1) PRIMARY KEY,
    name NVARCHAR(255) NOT NULL,
    price DECIMAL(10, 2) NOT NULL,
    stock INT DEFAULT 0,
    category NVARCHAR(100),
    created_at DATETIME2 DEFAULT GETUTCDATE()
);
GO

-- Insert test users
INSERT INTO dbo.users (name, email) VALUES
    (N'Alice Johnson', N'alice@example.com'),
    (N'Bob Smith', N'bob@example.com'),
    (N'Charlie Brown', N'charlie@example.com'),
    (N'Diana Prince', N'diana@example.com'),
    (N'Eve Wilson', N'eve@example.com');
GO

-- Insert test posts
INSERT INTO dbo.posts (title, content, author_id, published) VALUES
    (N'Introduction to FraiseQL', N'FraiseQL is a compiled GraphQL engine...', 1, 1),
    (N'Advanced Query Patterns', N'Learn about complex queries...', 1, 1),
    (N'Database Optimization', N'Tips for optimizing your database...', 2, 1),
    (N'Draft Post', N'This is a draft...', 3, 0);
GO

-- Insert test products
INSERT INTO dbo.products (name, price, stock, category) VALUES
    (N'Widget A', 19.99, 100, N'Electronics'),
    (N'Gadget B', 49.99, 50, N'Electronics'),
    (N'Tool C', 29.99, 75, N'Tools'),
    (N'Device D', 99.99, 25, N'Electronics');
GO

-- Create v_user view returning JSON in data column
CREATE VIEW dbo.v_user AS
SELECT
    id,
    (
        SELECT
            u.id,
            u.name,
            u.email,
            CONVERT(VARCHAR(30), u.created_at, 127) AS created_at
        FOR JSON PATH, WITHOUT_ARRAY_WRAPPER
    ) AS data
FROM dbo.users u;
GO

-- Create v_post view with nested author JSON
CREATE VIEW dbo.v_post AS
SELECT
    p.id,
    (
        SELECT
            p.id,
            p.title,
            p.content,
            p.author_id,
            p.published,
            CONVERT(VARCHAR(30), p.created_at, 127) AS created_at,
            (
                SELECT
                    u.id,
                    u.name,
                    u.email
                FROM dbo.users u
                WHERE u.id = p.author_id
                FOR JSON PATH, WITHOUT_ARRAY_WRAPPER
            ) AS author
        FOR JSON PATH, WITHOUT_ARRAY_WRAPPER
    ) AS data
FROM dbo.posts p;
GO

-- Create v_product view returning JSON in data column
CREATE VIEW dbo.v_product AS
SELECT
    id,
    (
        SELECT
            pr.id,
            pr.name,
            pr.price,
            pr.stock,
            pr.category,
            CONVERT(VARCHAR(30), pr.created_at, 127) AS created_at
        FOR JSON PATH, WITHOUT_ARRAY_WRAPPER
    ) AS data
FROM dbo.products pr;
GO

-- Create analytics tables
CREATE TABLE dbo.tf_sales (
    id BIGINT IDENTITY(1,1) PRIMARY KEY,
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,
    discount DECIMAL(10,2) DEFAULT 0.00,
    data NVARCHAR(MAX) NOT NULL,
    customer_id INT NOT NULL,
    product_id INT NOT NULL,
    occurred_at DATETIME2 NOT NULL,
    created_at DATETIME2 DEFAULT GETUTCDATE()
);
GO

CREATE INDEX idx_sales_customer ON dbo.tf_sales(customer_id);
CREATE INDEX idx_sales_product ON dbo.tf_sales(product_id);
CREATE INDEX idx_sales_occurred ON dbo.tf_sales(occurred_at);
GO

-- Insert sales data
INSERT INTO dbo.tf_sales (revenue, quantity, cost, discount, data, customer_id, product_id, occurred_at) VALUES
    (999.99, 1, 700.00, 0.00, N'{"category": "electronics", "region": "US", "channel": "online"}', 1, 1, '2024-01-15 10:30:00'),
    (29.99, 2, 15.00, 5.00, N'{"category": "electronics", "region": "UK", "channel": "online"}', 2, 2, '2024-01-16 14:20:00'),
    (999.99, 1, 700.00, 100.00, N'{"category": "electronics", "region": "FR", "channel": "store"}', 1, 1, '2024-01-17 09:15:00'),
    (299.99, 1, 180.00, 0.00, N'{"category": "furniture", "region": "US", "channel": "store"}', 3, 3, '2024-01-18 11:45:00'),
    (199.99, 2, 120.00, 20.00, N'{"category": "furniture", "region": "DE", "channel": "online"}', 4, 4, '2024-01-19 16:30:00'),
    (299.99, 1, 180.00, 30.00, N'{"category": "furniture", "region": "JP", "channel": "online"}', 5, 3, '2024-01-20 08:00:00'),
    (29.99, 5, 15.00, 0.00, N'{"category": "electronics", "region": "US", "channel": "online"}', 2, 2, '2024-01-21 13:25:00'),
    (999.99, 1, 700.00, 50.00, N'{"category": "electronics", "region": "UK", "channel": "store"}', 3, 1, '2024-01-22 10:10:00');
GO

CREATE TABLE dbo.tf_events (
    id BIGINT IDENTITY(1,1) PRIMARY KEY,
    duration_ms BIGINT NOT NULL,
    error_count INT DEFAULT 0,
    request_size BIGINT DEFAULT 0,
    response_size BIGINT DEFAULT 0,
    data NVARCHAR(MAX) NOT NULL,
    user_id INT,
    endpoint VARCHAR(255) NOT NULL,
    status_code INT NOT NULL,
    occurred_at DATETIME2 NOT NULL,
    created_at DATETIME2 DEFAULT GETUTCDATE()
);
GO

CREATE INDEX idx_events_user ON dbo.tf_events(user_id);
CREATE INDEX idx_events_endpoint ON dbo.tf_events(endpoint);
CREATE INDEX idx_events_status ON dbo.tf_events(status_code);
CREATE INDEX idx_events_occurred ON dbo.tf_events(occurred_at);
GO

-- Insert events data
INSERT INTO dbo.tf_events (duration_ms, error_count, request_size, response_size, data, user_id, endpoint, status_code, occurred_at) VALUES
    (150, 0, 512, 2048, N'{"method": "GET", "version": "v1", "client": "web"}', 1, '/api/users', 200, '2024-01-15 10:00:00'),
    (250, 0, 1024, 4096, N'{"method": "POST", "version": "v1", "client": "mobile"}', 2, '/api/users', 201, '2024-01-15 10:05:00'),
    (50, 1, 256, 128, N'{"method": "GET", "version": "v1", "client": "web"}', 3, '/api/posts', 404, '2024-01-15 10:10:00'),
    (180, 0, 768, 3072, N'{"method": "GET", "version": "v2", "client": "web"}', 1, '/api/posts', 200, '2024-01-15 10:15:00'),
    (5000, 1, 512, 256, N'{"method": "POST", "version": "v1", "client": "mobile"}', 4, '/api/orders', 500, '2024-01-15 10:20:00');
GO
EOF

echo "SQL Server initialization complete!"

# Wait for SQL Server process
wait $SQLSERVER_PID
