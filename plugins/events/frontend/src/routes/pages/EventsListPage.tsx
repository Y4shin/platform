import { useQuery } from '@connectrpc/connect-query';
import { Button, Card, Stack } from '@junius/design';
import { rpc } from '@junius/generated/events/rpc';

import { EventCard } from '../../lib/EventCard.js';
import { usePluginNavigate } from '../../nav.js';

/** The events landing page: every event the caller can see, plus a "New event"
 * action. Public events appear to everyone; private ones only to those with
 * access. */
export function EventsListPage() {
  const nav = usePluginNavigate();
  const { data, error, isPending } = useQuery(rpc.EventService.listEvents, {});

  return (
    <Stack gap="md">
      <div className="flex items-center justify-between">
        <h1 className="font-semibold text-xl">Events</h1>
        <Button onClick={() => nav('/p/events/new')}>New event</Button>
      </div>

      {error ? (
        <Card>
          <p className="text-danger text-sm">{String(error)}</p>
        </Card>
      ) : isPending ? (
        <Card>
          <p className="text-fg-2 text-sm">Loading…</p>
        </Card>
      ) : data.events.length === 0 ? (
        <Card>
          <p className="text-fg-2 text-sm">No events yet. Create your first one.</p>
        </Card>
      ) : (
        data.events.map((event) => (
          <EventCard
            key={event.id}
            event={event}
            onSelect={() => nav('/p/events/$eventId', { eventId: event.id })}
          />
        ))
      )}
    </Stack>
  );
}
