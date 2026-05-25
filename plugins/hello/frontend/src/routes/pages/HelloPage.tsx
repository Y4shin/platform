import { useQuery } from '@connectrpc/connect-query';
import { Card, Stack } from '@junius/design';
import { rpc } from '@junius/generated/hello/rpc';
import { useUser } from '@junius/sdk';

export function HelloPage() {
  const user = useUser();
  const name = user?.displayName ?? 'world';

  const { data, error, isPending } = useQuery(rpc.HelloService.greet, { name });

  return (
    <Stack gap="md">
      <Card>
        <Stack gap="sm">
          <h1 className="text-xl font-semibold">Hello, {name} 👋</h1>
          <p className="text-fg-2 text-sm">
            This page is contributed by the <code>hello</code> plugin. It calls{' '}
            <code>HelloService.Greet</code> on the backend over Connect-RPC.
          </p>
        </Stack>
      </Card>
      <Card>
        <Stack gap="sm">
          <h2 className="text-base font-semibold">Server reply (RPC)</h2>
          {error ? (
            <pre className="text-danger text-sm">{String(error)}</pre>
          ) : isPending ? (
            <p className="text-fg-2 text-sm">fetching…</p>
          ) : (
            <pre className="text-sm">{data?.message}</pre>
          )}
        </Stack>
      </Card>
    </Stack>
  );
}
