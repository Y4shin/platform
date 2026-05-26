import { useQuery } from '@connectrpc/connect-query';
import { Button, Card, Stack } from '@junius/design';
import { rpc } from '@junius/generated/events/rpc';
import { Trans } from '@lingui/react/macro';

import { useEventsError } from '../../errors.js';
import { EventCard } from '../../lib/EventCard.js';
import { usePluginNavigate } from '../../nav.js';

/** The events landing page: every event the caller can see, plus a "New event"
 * action. Public events appear to everyone; private ones only to those with
 * access. */
export function EventsListPage() {
  const nav = usePluginNavigate();
  const { data, error, isPending } = useQuery(rpc.EventService.listEvents, {});
  const renderError = useEventsError();

  return (
    <Stack gap="md">
      <div className="flex items-center justify-between">
        <h1 className="font-semibold text-xl">
          <Trans>Events</Trans>
        </h1>
        <Button onClick={() => nav('/p/events/new')}>
          <Trans>New event</Trans>
        </Button>
      </div>

      {error ? (
        <Card>
          <p className="text-danger text-sm">{renderError(error)}</p>
        </Card>
      ) : isPending ? (
        <Card>
          <p className="text-fg-2 text-sm">
            <Trans>Loading…</Trans>
          </p>
        </Card>
      ) : data.events.length === 0 ? (
        <Card>
          <p className="text-fg-2 text-sm">
            <Trans>No events yet. Create your first one.</Trans>
          </p>
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
