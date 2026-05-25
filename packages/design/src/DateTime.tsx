import { Input, type InputProps } from './Input.js';

export interface DateTimeInputProps extends Omit<InputProps, 'type'> {
  /** Render a date-only picker (vs. date + time). */
  allDay?: boolean;
}

/** A native date/time picker — `datetime-local`, or `date` when `allDay`. */
export function DateTimeInput({ allDay, ...rest }: DateTimeInputProps) {
  return <Input type={allDay ? 'date' : 'datetime-local'} {...rest} />;
}

/** A start/end range plus an all-day toggle. The values are the raw native input
 * strings (`YYYY-MM-DDTHH:mm`, or `YYYY-MM-DD` when all-day); callers convert
 * to/from their wire format (e.g. RFC 3339). */
export interface DateRangeValue {
  start: string;
  end: string;
  allDay: boolean;
}

export interface DateRangeProps {
  value: DateRangeValue;
  onChange: (value: DateRangeValue) => void;
  disabled?: boolean;
}

/** A controlled start→end date/time range with an all-day toggle. */
export function DateRange({ value, onChange, disabled }: DateRangeProps) {
  const dayOnly = (s: string) => s.split('T')[0] ?? '';
  const withTime = (s: string) => (s === '' || s.includes('T') ? s : `${s}T00:00`);

  const setAllDay = (allDay: boolean) => {
    const convert = allDay ? dayOnly : withTime;
    onChange({ allDay, start: convert(value.start), end: convert(value.end) });
  };

  return (
    <div className="flex flex-col gap-2">
      <label className="inline-flex items-center gap-2 text-sm text-fg-1">
        <input
          type="checkbox"
          checked={value.allDay}
          disabled={disabled}
          onChange={(e) => setAllDay(e.target.checked)}
        />
        All day
      </label>
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
        <DateTimeInput
          allDay={value.allDay}
          aria-label="Start"
          value={value.start}
          disabled={disabled}
          onChange={(e) => onChange({ ...value, start: e.target.value })}
        />
        <span className="text-sm text-fg-2">to</span>
        <DateTimeInput
          allDay={value.allDay}
          aria-label="End"
          value={value.end}
          disabled={disabled}
          onChange={(e) => onChange({ ...value, end: e.target.value })}
        />
      </div>
    </div>
  );
}
