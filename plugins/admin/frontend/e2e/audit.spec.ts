import { expect, test } from '@junius/e2e';

test.describe('audit log viewer', () => {
  test('renders audit table with seeded events, filters narrow rows, row click opens details drawer', async ({
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

    // Seed audit events so the page has data.
    const groupId = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa';
    await db.query(
      `INSERT INTO platform.audit_event (event_kind, actor_user_id, resource_kind, resource_id, details)
       VALUES ('admin:group.create', $1, 'platform:group', $2::uuid, '{"name":"Marketing"}')`,
      [alice.id, groupId],
    );
    await db.query(
      `INSERT INTO platform.audit_event (event_kind, actor_user_id, resource_kind, resource_id, details)
       VALUES ('admin:membership.add', $1, 'platform:group_membership', $2::uuid, '{"user_id":"bob"}')`,
      [alice.id, groupId],
    );

    // Navigate to the audit page.
    await page.goto('/p/admin/audit');

    // Table columns are visible.
    await expect(page.getByRole('columnheader', { name: 'When' })).toBeVisible();
    await expect(page.getByRole('columnheader', { name: 'Actor' })).toBeVisible();
    await expect(page.getByRole('columnheader', { name: 'Event' })).toBeVisible();
    await expect(page.getByRole('columnheader', { name: 'Resource' })).toBeVisible();

    // Both seeded events appear.
    await expect(page.getByText('admin:group.create')).toBeVisible();
    await expect(page.getByText('admin:membership.add')).toBeVisible();

    // Filter by event_kind narrows to one row.
    await page.getByPlaceholder('e.g. admin:membership.add').fill('admin:membership.add');
    await page.getByRole('button', { name: 'Filter' }).click();

    await expect(page.getByText('admin:membership.add')).toBeVisible();
    await expect(page.getByText('admin:group.create')).not.toBeVisible();

    // Click the row to open the details drawer.
    await page.getByText('admin:membership.add').click();
    await expect(page.getByText('Event details')).toBeVisible();
    // The JSON details contain "bob".
    await expect(page.getByText('"bob"')).toBeVisible();
  });
});
