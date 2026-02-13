-- SQL Server initialization script for FraiseQL integration tests
-- Creates test tables and NVARCHAR(MAX) JSON views
-- Equivalent to PostgreSQL init.sql but using SQL Server T-SQL

-- Create database if it doesn't exist
IF NOT EXISTS (SELECT * FROM sys.databases WHERE name = 'fraiseql_test')
BEGIN
  CREATE DATABASE fraiseql_test;
END
GO

USE fraiseql_test;
GO

-- Drop existing objects if they exist
IF OBJECT_ID('dbo.v_product', 'V') IS NOT NULL DROP VIEW dbo.v_product;
IF OBJECT_ID('dbo.v_post', 'V') IS NOT NULL DROP VIEW dbo.v_post;
IF OBJECT_ID('dbo.v_user', 'V') IS NOT NULL DROP VIEW dbo.v_user;
IF OBJECT_ID('dbo.posts', 'U') IS NOT NULL DROP TABLE dbo.posts;
IF OBJECT_ID('dbo.users', 'U') IS NOT NULL DROP TABLE dbo.users;
IF OBJECT_ID('dbo.products', 'U') IS NOT NULL DROP TABLE dbo.products;
GO

-- Create users table
CREATE TABLE dbo.users (
    id INT IDENTITY(1,1) PRIMARY KEY,
    name NVARCHAR(255) NOT NULL,
    email NVARCHAR(255) NOT NULL UNIQUE,
    created_at DATETIME2 DEFAULT GETUTCDATE()
);
GO

-- Create posts table with foreign key to users
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
-- SQL Server uses FOR JSON to construct JSON objects
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

-- Verify views work
-- SELECT TOP 3 * FROM dbo.v_user;
-- SELECT TOP 3 * FROM dbo.v_post;
-- SELECT TOP 3 * FROM dbo.v_product;
