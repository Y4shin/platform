import { Card, Stack } from '@junius/design';

import { formatWhen } from '../domain.js';

/** The minimal event shape `EventCard` renders — structurally satisfied by the
 * proto `Event` message, so callers can pass RPC results directly. */
export interface EventSummary {
  title: string;
  startsAt: string;
  endsAt?: string;
  allDay?: boolean;
  location?: string;
  visibility?: string;
}

export interface EventCardProps {
  event: EventSummary;
  /** Optional click handler (e.g. navigate to the event detail). */
  onSelect?: () => void;
}

/**
 * A compact event summary card (title, when, where, a public/private badge).
 * Exposed via `[exposes.components.EventCard]` for cross-plugin reuse; also used
 * by the events list page.
 */
export function EventCard({ event, onSelect }: EventCardProps) {
  const when = formatWhen(event.startsAt, event.endsAt || undefined, event.allDay ?? false);
  return (
    <Card>
      <button
        type="button"
        onClick={onSelect}
        disabled={!onSelect}
        className="w-full text-left disabled:cursor-default"
      >
        <Stack gap="sm">
          <div className="flex items-center justify-between gap-2">
            <h3 className="font-semibold text-fg-1">{event.title}</h3>
            {event.visibility && (
              <span className="rounded bg-surface-2 px-2 py-0.5 text-fg-2 text-xs">
                {event.visibility}
              </span>
            )}
          </div>
          {when && <p className="text-fg-2 text-sm">{when}</p>}
          {event.location && <p className="text-fg-2 text-sm">📍 {event.location}</p>}
        </Stack>
      </button>
    </Card>
  );
}
