import { expect, test } from '@junius/e2e';

// M18 admin flow: alice (assigned the built-in `admin` user-role) creates a
// group, adds a role with permissions, and adds bob as a member; bob then
// inherits those permissions. Asserted against the DB (the source of truth the
// host reads at login) rather than bob's `/api/me`, because `loginAs(bob)`
// would clobber alice's session cookie on the shared browser context.
test.describe('admin flow', () => {
  test('alice (admin user-role) sees /api/me carry the wildcard grant', async ({
    page,
    loginAs,
    db,
  }) => {
    const user = await loginAs('alice');
    // The harness's loginAs doesn't currently assign user-roles. Seed
    // alice as admin via the platform.user_role_assignment table
    // directly — the same way `dev/provisioning.toml` does in real
    // deployments, just inline here.
    await db.query(
      `INSERT INTO platform.user_role_assignment (user_id, role_id)
         SELECT $1, id FROM platform.user_role WHERE name = 'admin'
         ON CONFLICT DO NOTHING`,
      [user.id],
    );

    const res = await page.request.get('/api/me');
    expect(res.status()).toBe(200);
    const body = (await res.json()) as {
      userRoles?: { roleName: string; permissions: string[] }[];
    };
    const admin = body.userRoles?.find((r) => r.roleName === 'admin');
    expect(admin, 'alice carries the admin user-role').toBeDefined();
    expect(admin?.permissions).toContain('*');
  });

  test('alice creates a group, adds a role with permissions, adds bob — bob inherits the permissions', async ({
    page,
    loginAs,
    db,
  }) => {
    const alice = await loginAs('alice');
    await db.query(
      `INSERT INTO platform.user_role_assignment (user_id, role_id)
         SELECT $1, id FROM platform.user_role WHERE name = 'admin'
         ON CONFLICT DO NOTHING`,
      [alice.id],
    );
    // bob must exist (findUserByEmail resolves the membership target). Seed him
    // directly so we don't overwrite alice's session cookie on this context.
    const bob = await db.query<{ id: string }>(
      `INSERT INTO platform."user" (oidc_sub, email, display_name)
         VALUES ('e2e:bob', 'bob@e2e.local', 'Bob')
         ON CONFLICT (oidc_sub) DO UPDATE SET email = EXCLUDED.email
         RETURNING id`,
    );
    const bobId = bob.rows[0]?.id;
    expect(bobId, 'bob seeded').toBeTruthy();

    // 1. /p/admin → create group "Marketing".
    await page.goto('/p/admin');
    await page.getByPlaceholder('Group name').fill('Marketing');
    await page.getByRole('button', { name: 'Create' }).click();

    // 2. Open Marketing → add role "editor".
    await page.getByRole('button', { name: 'Marketing' }).click();
    await expect(page.getByRole('heading', { name: 'Marketing' })).toBeVisible();
    await page.getByPlaceholder('New role name').fill('editor');
    await page.getByRole('button', { name: 'Add role' }).click();
    // The new role renders with its own PermissionPicker; waiting for a
    // checkbox is both the readiness signal and the next step's target
    // (and avoids the ambiguous "editor" text, which also names the member
    // role <option>).
    await expect(page.getByRole('checkbox', { name: 'events:read' })).toBeVisible();

    // 3. Tick events:read + events:write in the role's PermissionPicker, save.
    await page.getByRole('checkbox', { name: 'events:read' }).check();
    await page.getByRole('checkbox', { name: 'events:write' }).check();
    await page.getByRole('button', { name: 'Save permissions' }).click();
    await expect(page.getByText('unsaved changes')).toHaveCount(0);

    // 4. Add bob as 'editor'. Scope to the member-role select (the page also
    // has the header LocaleSwitcher combobox) via its "Choose a role…" option.
    await page.getByPlaceholder('user@example.com').fill('bob@e2e.local');
    const roleSelect = page
      .getByRole('combobox')
      .filter({ has: page.getByRole('option', { name: 'Choose a role' }) });
    await roleSelect.selectOption({ label: 'editor' });
    await page.getByRole('button', { name: 'Add', exact: true }).click();
    await expect(page.getByText('bob@e2e.local')).toBeVisible();

    // 5. Assert bob's inherited membership + permissions (what the host reads).
    const membership = await db.query<{ role_name: string }>(
      `SELECT gr.name AS role_name
         FROM platform.group_membership gm
         JOIN platform."group" g  ON g.id  = gm.group_id
         JOIN platform.group_role gr ON gr.id = gm.role_id
        WHERE gm.user_id = $1 AND g.name = 'Marketing'`,
      [bobId],
    );
    expect(membership.rows[0]?.role_name).toBe('editor');

    const perms = await db.query<{ permission: string }>(
      `SELECT rp.permission
         FROM platform.role_permission rp
         JOIN platform.group_role gr ON gr.id = rp.role_id
         JOIN platform."group" g     ON g.id  = gr.group_id
        WHERE g.name = 'Marketing' AND gr.name = 'editor'`,
    );
    const granted = perms.rows.map((r) => r.permission);
    expect(granted).toContain('events:read');
    expect(granted).toContain('events:write');

    // 6. Every mutation is audited (group + role + set-perms + member add).
    const audit = await db.query<{ n: string }>(
      `SELECT count(*)::text AS n FROM platform.audit_event WHERE event_kind LIKE 'admin:%'`,
    );
    expect(Number(audit.rows[0]?.n ?? '0')).toBeGreaterThanOrEqual(4);
  });
});
