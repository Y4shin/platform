-- Dev-only convenience seed: give the Authentik test users (alice, bob) a group
-- role that grants every plugin permission, so the dev app is fully usable in the
-- browser. The platform has no built-in permission seed — a fresh OIDC user has
-- no group memberships and therefore no permissions, so the permission-gated
-- pages (events CRUD, …) would 403.
--
-- Idempotent; safe to re-run. Memberships only attach once the user rows exist,
-- so run this AFTER logging in once as each user (login creates platform.user).
--
--   docker compose -f dev/docker-compose.yml exec -T postgres \
--     psql -U platform_migrator -d platform < dev/dev-seed.sql

DO $$
DECLARE
  gid uuid;
  rid uuid;
BEGIN
  SELECT id INTO gid FROM platform.group WHERE name = 'Organisers' LIMIT 1;
  IF gid IS NULL THEN
    INSERT INTO platform.group (name, description)
    VALUES ('Organisers', 'Dev test group (all permissions)')
    RETURNING id INTO gid;
  END IF;

  SELECT id INTO rid FROM platform.group_role WHERE group_id = gid AND name = 'organiser' LIMIT 1;
  IF rid IS NULL THEN
    INSERT INTO platform.group_role (group_id, name) VALUES (gid, 'organiser') RETURNING id INTO rid;
  END IF;

  INSERT INTO platform.role_permission (role_id, permission)
  SELECT rid, p FROM unnest(ARRAY[
    'events:read', 'events:write', 'events:share'
  ]) AS p
  ON CONFLICT (role_id, permission) DO NOTHING;

  INSERT INTO platform.group_membership (user_id, group_id, role_id)
  SELECT u.id, gid, rid FROM platform.user u
  WHERE u.email IN ('alice@example.com', 'bob@example.com')
  ON CONFLICT (user_id, group_id) DO UPDATE SET role_id = EXCLUDED.role_id;
END $$;

-- Report what the test users now hold.
SELECT u.email, gr.name AS role, count(rp.permission) AS permissions
FROM platform.user u
JOIN platform.group_membership gm ON gm.user_id = u.id
JOIN platform.group_role gr ON gr.id = gm.role_id
LEFT JOIN platform.role_permission rp ON rp.role_id = gr.id
WHERE u.email IN ('alice@example.com', 'bob@example.com')
GROUP BY u.email, gr.name;
