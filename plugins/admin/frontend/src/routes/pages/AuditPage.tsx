import { useQuery } from '@connectrpc/connect-query';
import { Button, Card, Input, Stack } from '@junius/design';
import { rpc } from '@junius/generated/admin/rpc';
import { Trans, useLingui } from '@lingui/react/macro';
import { useState } from 'react';

export function AuditPage() {
  const { t } = useLingui();

  const [eventKind, setEventKind] = useState('');
  const [resourceKind, setResourceKind] = useState('');
  const [actorUserId, setActorUserId] = useState('');
  const [cursor, setCursor] = useState<string | undefined>(undefined);

  const list = useQuery(rpc.AuditService.list, {
    eventKind: eventKind || undefined,
    resourceKind: resourceKind || undefined,
    actorUserId: actorUserId || undefined,
    cursor,
    limit: 50,
  });

  type AuditEventRow = NonNullable<typeof list.data>['events'][number];
  const [drawerEvent, setDrawerEvent] = useState<AuditEventRow | null>(null);

  const applyFilter = () => {
    setCursor(undefined);
    list.refetch();
  };

  return (
    <Stack gap="md">
      <h1 className="font-semibold text-xl">
        <Trans>Audit log</Trans>
      </h1>

      <Card>
        <form
          className="flex flex-wrap gap-2 items-end"
          onSubmit={(e) => {
            e.preventDefault();
            applyFilter();
          }}
        >
          <div className="flex flex-col gap-1 text-sm">
            <label htmlFor="audit-event-kind">
              <Trans>Event kind</Trans>
            </label>
            <Input
              id="audit-event-kind"
              value={eventKind}
              onChange={(e) => setEventKind(e.currentTarget.value)}
              placeholder={t`e.g. admin:membership.add`}
            />
          </div>
          <div className="flex flex-col gap-1 text-sm">
            <label htmlFor="audit-resource-kind">
              <Trans>Resource kind</Trans>
            </label>
            <Input
              id="audit-resource-kind"
              value={resourceKind}
              onChange={(e) => setResourceKind(e.currentTarget.value)}
              placeholder={t`e.g. platform:group`}
            />
          </div>
          <div className="flex flex-col gap-1 text-sm">
            <label htmlFor="audit-actor">
              <Trans>Actor user ID</Trans>
            </label>
            <Input
              id="audit-actor"
              value={actorUserId}
              onChange={(e) => setActorUserId(e.currentTarget.value)}
              placeholder={t`UUID`}
            />
          </div>
          <Button type="submit">
            <Trans>Filter</Trans>
          </Button>
        </form>
      </Card>

      <Card>
        {list.error ? (
          <p className="text-danger text-sm">{list.error.message}</p>
        ) : list.isPending ? (
          <p className="text-fg-2 text-sm">
            <Trans>Loading…</Trans>
          </p>
        ) : list.data.events.length === 0 ? (
          <p className="text-fg-2 text-sm">
            <Trans>No audit events found.</Trans>
          </p>
        ) : (
          <>
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-left text-fg-2">
                  <th className="pb-2 pr-3">
                    <Trans>When</Trans>
                  </th>
                  <th className="pb-2 pr-3">
                    <Trans>Actor</Trans>
                  </th>
                  <th className="pb-2 pr-3">
                    <Trans>Event</Trans>
                  </th>
                  <th className="pb-2 pr-3">
                    <Trans>Resource</Trans>
                  </th>
                </tr>
              </thead>
              <tbody>
                {list.data.events.map((ev) => (
                  <tr
                    key={ev.id}
                    className="border-b cursor-pointer hover:bg-bg-2"
                    onClick={() => setDrawerEvent(ev)}
                  >
                    <td className="py-2 pr-3 whitespace-nowrap">
                      {formatTimestamp(ev.occurredAt)}
                    </td>
                    <td className="py-2 pr-3">{ev.actorDisplayName || ev.actorEmail || '—'}</td>
                    <td className="py-2 pr-3">
                      <code className="text-xs">{ev.eventKind}</code>
                    </td>
                    <td className="py-2 pr-3">
                      {ev.resourceKind ? (
                        <>
                          <code className="text-xs">{ev.resourceKind}</code>
                          {ev.resourceId ? (
                            <span className="text-fg-2 text-xs ml-1">
                              {ev.resourceId.slice(0, 8)}
                            </span>
                          ) : null}
                        </>
                      ) : (
                        '—'
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>

            <div className="mt-3 flex gap-2">
              {cursor ? (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => {
                    setCursor(undefined);
                  }}
                >
                  <Trans>First page</Trans>
                </Button>
              ) : null}
              {list.data.nextCursor ? (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => {
                    setCursor(list.data?.nextCursor ?? undefined);
                  }}
                >
                  <Trans>Next page</Trans>
                </Button>
              ) : null}
            </div>
          </>
        )}
      </Card>

      {drawerEvent ? (
        <Card>
          <div className="flex items-center justify-between">
            <h2 className="font-medium text-base">
              <Trans>Event details</Trans>
            </h2>
            <button
              type="button"
              className="text-fg-2 hover:text-fg text-sm underline"
              onClick={() => setDrawerEvent(null)}
            >
              <Trans>Close</Trans>
            </button>
          </div>
          <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
            <dt className="text-fg-2">
              <Trans>ID</Trans>
            </dt>
            <dd>
              <code className="text-xs">{drawerEvent.id}</code>
            </dd>
            <dt className="text-fg-2">
              <Trans>Event kind</Trans>
            </dt>
            <dd>
              <code className="text-xs">{drawerEvent.eventKind}</code>
            </dd>
            <dt className="text-fg-2">
              <Trans>Actor</Trans>
            </dt>
            <dd>{drawerEvent.actorDisplayName || drawerEvent.actorEmail || '—'}</dd>
            <dt className="text-fg-2">
              <Trans>Resource</Trans>
            </dt>
            <dd>
              {drawerEvent.resourceKind || '—'}
              {drawerEvent.resourceId ? ` → ${drawerEvent.resourceId}` : ''}
            </dd>
            <dt className="text-fg-2">
              <Trans>When</Trans>
            </dt>
            <dd>{drawerEvent.occurredAt}</dd>
          </dl>
          <h3 className="font-medium text-sm mt-3">
            <Trans>Details (JSON)</Trans>
          </h3>
          <pre className="mt-1 max-h-60 overflow-auto rounded bg-bg-2 p-2 text-xs">
            {formatJson(drawerEvent.details)}
          </pre>
        </Card>
      ) : null}
    </Stack>
  );
}

function formatTimestamp(rfc3339: string): string {
  try {
    return new Date(rfc3339).toLocaleString();
  } catch {
    return rfc3339;
  }
}

function formatJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
