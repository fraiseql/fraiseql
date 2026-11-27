CREATE SCHEMA app;

CREATE TABLE app.tb_note (
    id uuid PRIMARY KEY,
    title text NOT NULL,
    price numeric NOT NULL,
    balance numeric NOT NULL,
    discount numeric
);

CREATE VIEW app.v_note AS
    SELECT id, jsonb_build_object(
        'id', id, 'title', title, 'price', price, 'balance', balance, 'discount', discount
    ) AS data
    FROM app.tb_note;
