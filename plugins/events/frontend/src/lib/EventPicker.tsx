import { useQuery } from '@connectrpc/connect-query';
import { Input, Select, Stack } from '@junius/design';
import { rpc } from '@junius/generated/events/rpc';
import { useLingui } from '@lingui/react/macro';
import { useMemo, useState } from 'react';

export interface EventPickerProps {
  /** Currently selected event id (controlled), or `null`. */
  value?: string | null;
  /** Called with the chosen event id (or `''` when cleared). */
  onChange?: (eventId: string) => void;
  label?: string;
}

/**
 * A searchable event selector: a text filter over `EventService.ListEvents` plus
 * a `<select>` of the matches. Exposed via `[exposes.components.EventPicker]` for
 * cross-plugin reuse.
 */
export function EventPicker({ value, onChange, label }: EventPickerProps) {
  const [filter, setFilter] = useState('');
  const { data } = useQuery(rpc.EventService.listEvents, {});
  const { t } = useLingui();
  const effectiveLabel = label ?? t`Event`;

  const options = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    return (data?.events ?? [])
      .filter((e) => !needle || e.title.toLowerCase().includes(needle))
      .map((e) => ({ value: e.id, label: e.title }));
  }, [data, filter]);

  return (
    <Stack gap="sm">
      <span className="font-medium text-fg-1 text-sm">{effectiveLabel}</span>
      <Input
        type="search"
        placeholder={t`Filter events…`}
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      <Select
        value={value ?? ''}
        placeholder={t`Select an event…`}
        options={options}
        onChange={(e) => onChange?.(e.target.value)}
      />
    </Stack>
  );
}
