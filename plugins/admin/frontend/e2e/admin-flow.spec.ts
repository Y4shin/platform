import { expect, test } from '@junius/e2e';

// M18 admin flow: alice (assigned the built-in `admin` user-role)
// creates a group, adds a role with permissions, adds bob as a member;
// bob's next `/api/me` reflects the new group + permissions. Marked
// `.fixme` pending UI-selector verification against the live host; the
// journey shape is captured below so a follow-up can flip it to live.
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

  test.fixme('alice creates a group, adds a role with permissions, adds bob — bob inherits the permissions', async ({
    page,
    loginAs,
  }) => {
    // 1. alice as admin → /p/admin → "Create group" → "Marketing" + description.
    // 2. Click into Marketing → "Add role" → "editor".
    //    Open the editor's PermissionPicker → tick `events:read`, `events:write`.
    // 3. "Add member" → bob@e2e.local + role "editor".
    // 4. bob's session: `loginAs('bob')` + GET /api/me; assert memberships[]
    //    contains { groupName: 'Marketing', role: { name: 'editor' },
    //    permissions ⊇ {'events:read','events:write'} }.
    // 5. psql: SELECT count(*) FROM platform.audit_event WHERE event_kind LIKE 'admin:%'
    //    is >= 5 (group.create, role.create, role.set_permissions, membership.add).
    await page.goto('/p/admin');
    void loginAs;
  });
});
