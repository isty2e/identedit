CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

INSERT INTO users (id, name) VALUES (1, 'identedit');
SELECT id, name FROM users WHERE id = 1;
