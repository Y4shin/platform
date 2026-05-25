import { Stack } from '@junius/design';
import { useId } from 'react';

/** Curated venue list. A real widget would fetch these; the demo keeps them static. */
export const VENUES = ['Main Hall', 'Garden Terrace', 'Rooftop Lounge', 'Lakeside Pavilion'];

export interface VenuePickerProps {
  /** Currently selected venue (controlled). */
  value?: string;
  /** Called with the chosen venue name. */
  onChange?: (venue: string) => void;
  /** Field label. */
  label?: string;
}

/**
 * A `<select>` over a curated venue list. The `widgets` plugin exposes this via
 * `[exposes.components.VenuePicker]`; consumers reach it at runtime through
 * `useComponent('widgets.VenuePicker')`. It is prop-compatible with a plain text
 * `<input value onChange>` so consumers can fall back to one when widgets is
 * disabled.
 */
export function VenuePicker({ value, onChange, label = 'Venue' }: VenuePickerProps) {
  const id = useId();
  return (
    <Stack gap="sm">
      <label className="text-sm font-medium" htmlFor={id}>
        {label}
      </label>
      <select
        id={id}
        className="border-fg-3 rounded border px-2 py-1 text-sm"
        value={value ?? ''}
        onChange={(e) => onChange?.(e.target.value)}
      >
        <option value="">Select a venue…</option>
        {VENUES.map((venue) => (
          <option key={venue} value={venue}>
            {venue}
          </option>
        ))}
      </select>
    </Stack>
  );
}
