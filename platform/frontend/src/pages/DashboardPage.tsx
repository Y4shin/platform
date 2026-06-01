import { Card, Stack } from '@junius/design';
import { Trans } from '@lingui/react/macro';
import { useQuery } from '@tanstack/react-query';

import { fetchMe, ME_QUERY_KEY, type MeResponse } from '../api/me.js';

// The `/` dashboard. v1 is intentionally minimal: a greeting for users who hold
// any access, and a no-access empty state for brand-new zero-permission
// accounts so they land on an explanation rather than a blank pane. The
// permission-filtered app-tile grid is deferred to M20 (once `[[plugin.apps]]`
// is the source of truth), so there are no tiles here.
export function DashboardPage() {
  const { data, isPending, isError } = useQuery({ queryKey: ME_QUERY_KEY, queryFn: fetchMe });

  if (isPending) {
    return (
      <p className="text-fg-2">
        <Trans>Loading…</Trans>
      </p>
    );
  }
  if (isError || !data) {
    return (
      <p className="text-danger">
        <Trans>Could not load your dashboard.</Trans>
      </p>
    );
  }
  return <DashboardView me={data} />;
}

// The pure render branch, exported for unit testing: any group membership or
// user-role means the account has access (→ greeting); none means a fresh
// zero-permission account (→ empty state).
export function DashboardView({ me }: { me: MeResponse }) {
  const hasAccess = me.memberships.length > 0 || me.userRoles.length > 0;
  return hasAccess ? (
    <Greeting name={me.displayName} />
  ) : (
    <NoAccessEmptyState adminContactEmail={me.adminContactEmail} />
  );
}

function Greeting({ name }: { name: string }) {
  return (
    <Stack gap="lg">
      <h1 className="font-semibold text-xl">
        <Trans>Welcome back, {name}</Trans>
      </h1>
    </Stack>
  );
}

// Shown when the viewer holds zero permissions. Names the deployment's
// `admin_contact_email` when configured, and degrades to generic copy (no
// contact line) when it is unset.
export function NoAccessEmptyState({ adminContactEmail }: { adminContactEmail: string | null }) {
  return (
    <Stack gap="lg">
      <h1 className="font-semibold text-xl">
        <Trans>You don't have access yet</Trans>
      </h1>
      <Card>
        <Stack gap="sm">
          <p className="text-fg-2 text-sm">
            <Trans>
              Your account doesn't have access to anything yet. Ask your administrator to add you to
              a group.
            </Trans>
          </p>
          {adminContactEmail ? (
            <p className="text-fg-2 text-sm">
              <Trans>Contact your administrator:</Trans>{' '}
              <a className="underline" href={`mailto:${adminContactEmail}`}>
                <code>{adminContactEmail}</code>
              </a>
            </p>
          ) : null}
        </Stack>
      </Card>
    </Stack>
  );
}
