-- SQLite initialization script for FraiseQL integration tests
-- Creates test tables and JSON views using SQLite JSON functions
-- Equivalent to PostgreSQL init.sql but using SQLite syntax

-- Drop existing objects (SQLite doesn't have IF EXISTS for views in older versions)
DROP VIEW IF EXISTS v_product;
DROP VIEW IF EXISTS v_post;
DROP VIEW IF EXISTS v_user;
DROP TABLE IF EXISTS posts;
DROP TABLE IF EXISTS users;
DROP TABLE IF EXISTS products;

-- Create users table
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    created_at TEXT DEFAULT (datetime('now'))
);

-- Create posts table with foreign key to users
CREATE TABLE posts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    content TEXT,
    author_id INTEGER NOT NULL REFERENCES users(id),
    published INTEGER DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now'))
);

-- Create products table
CREATE TABLE products (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    price REAL NOT NULL,
    stock INTEGER DEFAULT 0,
    category TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

-- Insert test users
INSERT INTO users (name, email) VALUES
    ('Alice Johnson', 'alice@example.com'),
    ('Bob Smith', 'bob@example.com'),
    ('Charlie Brown', 'charlie@example.com'),
    ('Diana Prince', 'diana@example.com'),
    ('Eve Wilson', 'eve@example.com');

-- Insert test posts
INSERT INTO posts (title, content, author_id, published) VALUES
    ('Introduction to FraiseQL', 'FraiseQL is a compiled GraphQL engine...', 1, 1),
    ('Advanced Query Patterns', 'Learn about complex queries...', 1, 1),
    ('Database Optimization', 'Tips for optimizing your database...', 2, 1),
    ('Draft Post', 'This is a draft...', 3, 0);

-- Insert test products
INSERT INTO products (name, price, stock, category) VALUES
    ('Widget A', 19.99, 100, 'Electronics'),
    ('Gadget B', 49.99, 50, 'Electronics'),
    ('Tool C', 29.99, 75, 'Tools'),
    ('Device D', 99.99, 25, 'Electronics');

-- Create v_user view returning JSON in data column
-- SQLite uses json_object() for constructing JSON
CREATE VIEW v_user AS
SELECT
    id,
    json_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM users;

-- Create v_post view with nested author JSON
-- Note: SQLite requires subquery for nested objects
CREATE VIEW v_post AS
SELECT
    p.id,
    json_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author_id', p.author_id,
        'published', CASE WHEN p.published = 1 THEN json('true') ELSE json('false') END,
        'created_at', p.created_at,
        'author', json_object(
            'id', u.id,
            'name', u.name,
            'email', u.email
        )
    ) AS data
FROM posts p
JOIN users u ON p.author_id = u.id;

-- Create v_product view returning JSON in data column
CREATE VIEW v_product AS
SELECT
    id,
    json_object(
        'id', id,
        'name', name,
        'price', price,
        'stock', stock,
        'category', category,
        'created_at', created_at
    ) AS data
FROM products;

-- Verify views work
-- SELECT * FROM v_user LIMIT 3;
-- SELECT * FROM v_post LIMIT 3;
-- SELECT * FROM v_product LIMIT 3;
