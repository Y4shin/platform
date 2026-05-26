import { useQuery } from '@connectrpc/connect-query';
import { Card, Stack } from '@junius/design';
import { rpc } from '@junius/generated/hello/rpc';
import { useUser } from '@junius/sdk';
import { Trans, useLingui } from '@lingui/react/macro';

export function HelloPage() {
  const user = useUser();
  const { t } = useLingui();
  const name = user?.displayName ?? t`world`;

  const { data, error, isPending } = useQuery(rpc.HelloService.greet, { name });

  return (
    <Stack gap="md">
      <Card>
        <Stack gap="sm">
          <h1 className="text-xl font-semibold">
            <Trans>Hello, {name} 👋</Trans>
          </h1>
          <p className="text-fg-2 text-sm">
            <Trans>
              This page is contributed by the <code>hello</code> plugin. It calls{' '}
              <code>HelloService.Greet</code> on the backend over Connect-RPC.
            </Trans>
          </p>
        </Stack>
      </Card>
      <Card>
        <Stack gap="sm">
          <h2 className="text-base font-semibold">
            <Trans>Server reply (RPC)</Trans>
          </h2>
          {error ? (
            <pre className="text-danger text-sm">{String(error)}</pre>
          ) : isPending ? (
            <p className="text-fg-2 text-sm">
              <Trans>fetching…</Trans>
            </p>
          ) : (
            <pre className="text-sm">{data?.message}</pre>
          )}
        </Stack>
      </Card>
    </Stack>
  );
}
