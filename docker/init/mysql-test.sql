-- FraiseQL integration test data for MySQL
-- Loaded automatically by docker-compose.test.yml

-- ============================================================================
-- Users table + v_user view
-- ============================================================================
CREATE TABLE users (
    id    INT AUTO_INCREMENT PRIMARY KEY,
    data  JSON NOT NULL
);

INSERT INTO users (data) VALUES
('{"id": 1, "name": "Alice",   "email": "alice@example.com",   "role": "admin",     "age": 28, "active": true,  "metadata": {"city": "Paris",    "country": "FR"}}'),
('{"id": 2, "name": "Bob",     "email": "bob@example.com",     "role": "user",      "age": 25, "active": true,  "metadata": {"city": "London",   "country": "GB"}}'),
('{"id": 3, "name": "Charlie", "email": "charlie@example.com", "role": "moderator", "age": 35, "active": false, "metadata": {"city": "Berlin",   "country": "DE"}}'),
('{"id": 4, "name": "Diana",   "email": "diana@example.com",   "role": "user",      "age": 30, "active": true,  "metadata": {"city": "Paris",    "country": "FR"}}'),
('{"id": 5, "name": "Eve",     "email": "eve@example.com",     "role": "admin",     "age": 22, "active": true,  "metadata": {"city": "New York", "country": "US"}}');

CREATE VIEW v_user AS SELECT data FROM users;

-- ============================================================================
-- Posts table + v_post view
-- ============================================================================
CREATE TABLE posts (
    id    INT AUTO_INCREMENT PRIMARY KEY,
    data  JSON NOT NULL
);

INSERT INTO posts (data) VALUES
('{"id": 1, "title": "Hello World",     "author": {"id": 1, "name": "Alice", "email": "alice@example.com"},   "published": true,  "views": 100}'),
('{"id": 2, "title": "GraphQL Basics",  "author": {"id": 2, "name": "Bob",   "email": "bob@example.com"},     "published": true,  "views": 250}'),
('{"id": 3, "title": "Advanced Queries","author": {"id": 1, "name": "Alice", "email": "alice@example.com"},   "published": true,  "views": 75}'),
('{"id": 4, "title": "Draft Post",      "author": {"id": 3, "name": "Charlie","email": "charlie@example.com"},"published": false, "views": 0}');

CREATE VIEW v_post AS SELECT data FROM posts;
