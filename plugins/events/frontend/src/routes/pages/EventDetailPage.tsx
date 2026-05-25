import { useMutation, useQuery } from '@connectrpc/connect-query';
import { Button, Card, Stack } from '@junius/design';
import { rpc } from '@junius/generated/events/rpc';
import { useParams } from '@tanstack/react-router';

import { formatWhen } from '../../domain.js';
import { usePluginNavigate } from '../../nav.js';

/** A single event's detail, with owner actions (edit / delete / manage invite)
 * gated on the server-computed `viewerCanEdit` flag. */
export function EventDetailPage() {
  const { eventId } = useParams({ strict: false }) as { eventId?: string };
  const nav = usePluginNavigate();
  const { data, error, isPending } = useQuery(
    rpc.EventService.getEvent,
    { id: eventId ?? '' },
    { enabled: Boolean(eventId) },
  );
  const del = useMutation(rpc.EventService.deleteEvent, {
    onSuccess: () => nav('/p/events'),
  });

  if (error) {
    return (
      <Card>
        <p className="text-danger text-sm">{String(error)}</p>
      </Card>
    );
  }
  if (isPending || !data.event) {
    return (
      <Card>
        <p className="text-fg-2 text-sm">Loading…</p>
      </Card>
    );
  }

  const event = data.event;
  const when = formatWhen(event.startsAt, event.endsAt || undefined, event.allDay);

  return (
    <Stack gap="md">
      <Card>
        <Stack gap="sm">
          <div className="flex items-center justify-between gap-2">
            <h1 className="font-semibold text-xl">{event.title}</h1>
            <span className="rounded bg-surface-2 px-2 py-0.5 text-fg-2 text-xs">
              {event.visibility}
            </span>
          </div>
          {when && <p className="text-fg-2 text-sm">{when}</p>}
          {event.location && <p className="text-fg-2 text-sm">📍 {event.location}</p>}
          {event.description && <p className="text-fg-1 text-sm">{event.description}</p>}
        </Stack>
      </Card>

      {event.viewerCanEdit && (
        <Card>
          <Stack gap="sm" direction="row">
            <Button
              variant="secondary"
              onClick={() => nav('/p/events/$eventId/edit', { eventId: event.id })}
            >
              Edit
            </Button>
            <Button
              variant="secondary"
              onClick={() => nav('/p/events/$eventId/invite', { eventId: event.id })}
            >
              Manage invite
            </Button>
            <Button
              variant="ghost"
              disabled={del.isPending}
              onClick={() => del.mutate({ id: event.id })}
            >
              Delete
            </Button>
          </Stack>
        </Card>
      )}
    </Stack>
  );
}
