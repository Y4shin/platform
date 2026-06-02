import { Card, Stack } from '@junius/design';
import type { ForbiddenState } from '@junius/sdk';
import { Trans } from '@lingui/react/macro';
import { useQuery } from '@tanstack/react-query';
import { useLocation } from '@tanstack/react-router';

import { fetchMe, ME_QUERY_KEY } from '../api/me.js';

// The `/403` permission-denied page. Reached two ways, both carrying the
// missing permission names in location state when known: the `requirePermissions`
// route guard (direct-URL / stale-link navigation) and `queryClient.onError`
// on a `PermissionDenied` RPC. Names the missing permission(s) when present and
// falls back to generic copy otherwise (an RPC denial may not name them).
export function ForbiddenPage() {
  const state = useLocation({ select: (l) => l.state as ForbiddenState | undefined });
  // The viewer is authenticated (they have a session, just lack a permission),
  // so `/api/me` resolves; reuse it for the deployment contact address.
  const { data } = useQuery({ queryKey: ME_QUERY_KEY, queryFn: fetchMe });
  return (
    <ForbiddenView
      missing={state?.missing ?? []}
      adminContactEmail={data?.adminContactEmail ?? null}
    />
  );
}

// Pure render branch, exported for unit testing.
export function ForbiddenView({
  missing,
  adminContactEmail,
}: {
  missing: readonly string[];
  adminContactEmail: string | null;
}) {
  return (
    <Stack gap="lg">
      <h1 className="font-semibold text-xl">
        <Trans>Access denied</Trans>
      </h1>
      <Card>
        <Stack gap="sm">
          {missing.length > 0 ? (
            <p className="text-fg-2 text-sm">
              <Trans>You're missing the permission(s) needed for this page:</Trans>{' '}
              <code>{missing.join(', ')}</code>
            </p>
          ) : (
            <p className="text-fg-2 text-sm">
              <Trans>You don't have permission to view this page.</Trans>
            </p>
          )}
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
