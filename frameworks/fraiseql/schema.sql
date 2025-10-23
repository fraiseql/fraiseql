-- Benchmark database schema for GraphQL framework comparison
-- Based on FRAMEWORK_SUBMISSION_GUIDE requirements

CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    age INTEGER,
    city VARCHAR(255),
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    content TEXT,
    published BOOLEAN DEFAULT FALSE,
    author_id INTEGER REFERENCES users(id),
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE comments (
    id SERIAL PRIMARY KEY,
    content TEXT NOT NULL,
    post_id INTEGER REFERENCES posts(id),
    author_id INTEGER REFERENCES users(id),
    created_at TIMESTAMP DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_author_id ON comments(author_id);

-- Insert sample data for benchmarking
INSERT INTO users (name, email, age, city) VALUES
('Alice Johnson', 'alice@example.com', 28, 'New York'),
('Bob Smith', 'bob@example.com', 34, 'Los Angeles'),
('Carol Williams', 'carol@example.com', 29, 'Chicago'),
('David Brown', 'david@example.com', 42, 'Houston'),
('Eve Davis', 'eve@example.com', 31, 'Phoenix'),
('Frank Miller', 'frank@example.com', 38, 'Philadelphia'),
('Grace Wilson', 'grace@example.com', 26, 'San Antonio'),
('Henry Moore', 'henry@example.com', 45, 'San Diego'),
('Ivy Taylor', 'ivy@example.com', 33, 'Dallas'),
('Jack Anderson', 'jack@example.com', 27, 'San Jose');

INSERT INTO posts (title, content, published, author_id) VALUES
('Getting Started with GraphQL', 'GraphQL is a query language for APIs...', true, 1),
('Database Design Best Practices', 'When designing databases, consider...', true, 2),
('API Performance Optimization', 'Optimizing API performance requires...', true, 3),
('Modern Web Development', 'Modern web development has evolved...', true, 4),
('Data Modeling Techniques', 'Effective data modeling is crucial...', true, 5),
('REST vs GraphQL', 'Comparing REST and GraphQL approaches...', true, 1),
('Caching Strategies', 'Implementing effective caching strategies...', true, 2),
('Security in Web Applications', 'Security should be a primary concern...', true, 3),
('Scalability Patterns', 'Scaling web applications requires...', true, 4),
('Testing GraphQL APIs', 'Testing GraphQL APIs differs from REST...', true, 5);

INSERT INTO comments (content, post_id, author_id) VALUES
('Great article!', 1, 2),
('Very helpful, thanks!', 1, 3),
('I learned a lot from this.', 2, 1),
('Excellent explanation.', 2, 4),
('This clarified many concepts for me.', 3, 5),
('Well written and informative.', 3, 1),
('I agree with your points.', 4, 2),
('This is very useful information.', 4, 3),
('Thanks for sharing this knowledge.', 5, 4),
('Clear and concise explanation.', 5, 5);
