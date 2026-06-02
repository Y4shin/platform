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

    // Seed audit events so the page has data. The E2E specs share one
    // database (no per-test isolation) and Playwright retries re-run against
    // it, so other specs' audit rows — and our own prior attempts — coexist
    // here. Use run-unique event kinds so every assertion below targets
    // exactly our two rows regardless of what else is in the table.
    const run = crypto.randomUUID().slice(0, 8);
    const createKind = `e2e:audit.${run}.group.create`;
    const addKind = `e2e:audit.${run}.membership.add`;
    const groupId = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa';
    await db.query(
      `INSERT INTO platform.audit_event (event_kind, actor_user_id, resource_kind, resource_id, details)
       VALUES ($3, $1, 'platform:group', $2::uuid, '{"name":"Marketing"}')`,
      [alice.id, groupId, createKind],
    );
    await db.query(
      `INSERT INTO platform.audit_event (event_kind, actor_user_id, resource_kind, resource_id, details)
       VALUES ($3, $1, 'platform:group_membership', $2::uuid, '{"user_id":"bob"}')`,
      [alice.id, groupId, addKind],
    );

    // Navigate to the audit page.
    await page.goto('/p/admin/audit');

    // Table columns are visible.
    await expect(page.getByRole('columnheader', { name: 'When' })).toBeVisible();
    await expect(page.getByRole('columnheader', { name: 'Actor' })).toBeVisible();
    await expect(page.getByRole('columnheader', { name: 'Event' })).toBeVisible();
    await expect(page.getByRole('columnheader', { name: 'Resource' })).toBeVisible();

    // Both seeded events appear.
    await expect(page.getByText(createKind)).toBeVisible();
    await expect(page.getByText(addKind)).toBeVisible();

    // Filter by event_kind narrows to one row.
    await page.getByPlaceholder('e.g. admin:membership.add').fill(addKind);
    await page.getByRole('button', { name: 'Filter' }).click();

    await expect(page.getByText(addKind)).toBeVisible();
    await expect(page.getByText(createKind)).not.toBeVisible();

    // Click the row to open the details drawer.
    await page.getByText(addKind).click();
    await expect(page.getByText('Event details')).toBeVisible();
    // The JSON details contain "bob".
    await expect(page.getByText('"bob"')).toBeVisible();
  });
});
