-- @requires
-- M09: a read-mostly lookup table exposed to other plugins. `greetings` declares
-- a required dep on hello and references this via a NOT NULL cross-plugin FK; the
-- grants step GRANTs role_greetings SELECT on it (see crates/manifest grants).
CREATE TABLE hello.greeting_template (
    id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name  TEXT NOT NULL UNIQUE,
    body  TEXT NOT NULL
);

-- Seed two well-known templates so consumers have stable rows to reference.
INSERT INTO hello.greeting_template (name, body) VALUES
    ('default', 'Hello, {name}!'),
    ('formal',  'Good day, {name}.');
