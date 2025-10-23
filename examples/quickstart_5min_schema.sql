-- Simple notes table

CREATE TABLE tb_note (

    id SERIAL PRIMARY KEY,

    title VARCHAR(200) NOT NULL,

    content TEXT,

    created_at TIMESTAMP DEFAULT NOW()

);

-- Notes view for GraphQL queries

CREATE VIEW v_note AS

SELECT

    id,

    jsonb_build_object(

        'id', id,

        'title', title,

        'content', content,

        'created_at', created_at

    ) AS data

FROM tb_note;

-- Sample data

INSERT INTO tb_note (title, content) VALUES

    ('Welcome to FraiseQL', 'This is your first note!'),

    ('GraphQL is awesome', 'Queries and mutations made simple'),

    ('Database-first design', 'Views compose data for optimal performance');
