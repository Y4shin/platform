import { Card, Stack } from '@junius/design';
import { useUser } from '@junius/sdk';
import { useEffect, useState } from 'react';

export function HelloPage() {
  const user = useUser();
  const [serverReply, setServerReply] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch('/h/hello/ping')
      .then(async (r) => {
        if (!r.ok) {
          throw new Error(`HTTP ${r.status}`);
        }
        return r.text();
      })
      .then((text) => {
        if (!cancelled) {
          setServerReply(text);
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setError(String(e));
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <Stack gap="md">
      <Card>
        <Stack gap="sm">
          <h1 className="text-xl font-semibold">Hello, {user?.displayName ?? 'plugin'} 👋</h1>
          <p className="text-fg-2 text-sm">
            This page is contributed by the <code>hello</code> plugin. It calls
            <code>/h/hello/ping</code> on the host to prove the dev proxy works.
          </p>
        </Stack>
      </Card>
      <Card>
        <Stack gap="sm">
          <h2 className="text-base font-semibold">Server reply</h2>
          {error !== null ? (
            <pre className="text-danger text-sm">{error}</pre>
          ) : serverReply === null ? (
            <p className="text-fg-2 text-sm">fetching…</p>
          ) : (
            <pre className="text-sm">{serverReply}</pre>
          )}
        </Stack>
      </Card>
    </Stack>
  );
}
