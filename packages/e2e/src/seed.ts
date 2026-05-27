import { randomUUID } from 'node:crypto';
import pg from 'pg';

const { Pool } = pg;
export type DbPool = pg.Pool;

let cachedPool: DbPool | undefined;

/**
 * Connect (lazily, once per process) to the Postgres the host uses. The URL
 * comes from `DATABASE_URL`; the fixture forwards whatever the Playwright
 * `globalSetup` chose (an ephemeral testcontainers Postgres in Stage 3, or a
 * hand-started dev DB in Stage 1).
 */
export function pool(): DbPool {
  if (!cachedPool) {
    const connectionString = process.env.DATABASE_URL;
    if (!connectionString) {
      throw new Error(
        '@junius/e2e: DATABASE_URL is not set. Stage 1 expects a hand-started dev stack; ' +
          'Stage 3 wires this via Playwright globalSetup.',
      );
    }
    cachedPool = new Pool({ connectionString, max: 4 });
  }
  return cachedPool;
}

export async function closePool(): Promise<void> {
  if (cachedPool) {
    await cachedPool.end();
    cachedPool = undefined;
  }
}

export interface SeededUser {
  id: string;
  email: string;
  displayName: string;
}

/**
 * Idempotent user upsert. The host's session middleware doesn't care how the
 * row got there (no signing, no OIDC tokens required for plain session
 * lookup) — see platform/src/auth/session.rs. We use a synthetic `e2e:<name>`
 * subject so seeded users can't collide with real Authentik subjects.
 */
export async function upsertUser(name: string): Promise<SeededUser> {
  const email = `${name}@e2e.local`;
  const oidcSub = `e2e:${name}`;
  const displayName = name.charAt(0).toUpperCase() + name.slice(1);

  const result = await pool().query<{ id: string }>(
    `INSERT INTO platform."user" (oidc_sub, email, display_name)
       VALUES ($1, $2, $3)
       ON CONFLICT (oidc_sub) DO UPDATE
         SET email = EXCLUDED.email, display_name = EXCLUDED.display_name, updated_at = now()
       RETURNING id`,
    [oidcSub, email, displayName],
  );
  const id = result.rows[0]?.id;
  if (!id) throw new Error(`upsertUser(${name}): no row returned`);
  return { id, email, displayName };
}

/**
 * Create a fresh group + role + role_permissions per call and add the user.
 * Per-invocation isolation (UUID-named group) keeps tests from leaking permission
 * grants into each other when the DB is shared across specs.
 */
export async function grantPermissions(
  userId: string,
  permissions: readonly string[],
): Promise<void> {
  if (permissions.length === 0) return;

  const groupName = `e2e-${randomUUID()}`;
  const client = await pool().connect();
  try {
    await client.query('BEGIN');

    const groupRow = await client.query<{ id: string }>(
      `INSERT INTO platform."group" (name, description) VALUES ($1, $2) RETURNING id`,
      [groupName, 'E2E test fixture'],
    );
    const groupId = groupRow.rows[0]?.id;
    if (!groupId) throw new Error('grantPermissions: group insert returned no row');

    const roleRow = await client.query<{ id: string }>(
      `INSERT INTO platform.group_role (group_id, name) VALUES ($1, $2) RETURNING id`,
      [groupId, 'e2e'],
    );
    const roleId = roleRow.rows[0]?.id;
    if (!roleId) throw new Error('grantPermissions: role insert returned no row');

    await client.query(
      `INSERT INTO platform.role_permission (role_id, permission)
         SELECT $1, p FROM unnest($2::text[]) AS p`,
      [roleId, permissions],
    );

    await client.query(
      `INSERT INTO platform.group_membership (user_id, group_id, role_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, group_id) DO UPDATE SET role_id = EXCLUDED.role_id`,
      [userId, groupId, roleId],
    );

    await client.query('COMMIT');
  } catch (e) {
    await client.query('ROLLBACK');
    throw e;
  } finally {
    client.release();
  }
}

export async function insertSession(userId: string): Promise<string> {
  const row = await pool().query<{ id: string }>(
    `INSERT INTO platform.session (user_id, expires_at)
       VALUES ($1, now() + interval '1 day')
       RETURNING id`,
    [userId],
  );
  const id = row.rows[0]?.id;
  if (!id) throw new Error(`insertSession(${userId}): no row returned`);
  return id;
}
