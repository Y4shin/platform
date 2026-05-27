import { test } from '@junius/e2e';

// M13 walk #2 — group ownership. A group-owned event is visible to group
// members only (when private); non-members do not see it.
test.describe('group-owned event visibility', () => {
  test.fixme('a group event is visible to group members and hidden from non-members', async ({
    page,
    loginAs,
    db,
  }) => {
    // Setup notes:
    // - The fixture currently grants permissions via a *fresh, per-call*
    //   group. For this scenario we need a *shared* group that alice + bob
    //   both belong to, and carol does NOT. The cleanest path is a direct
    //   SQL seed via `db`:
    //     INSERT INTO platform."group" + group_role(events:read,events:write)
    //     + group_membership for alice + bob.
    //   Then loginAs() for each user separately.
    // - Create the event in alice's session with owner_kind=group, owning_group_id=<that group>.
    // - bob sees it on /p/events; carol does not.
    await page.goto('/p/events');
    void loginAs;
    void db;
  });
});
