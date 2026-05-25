//! Frontend domain helpers for the events plugin: display formatting and the
//! conversions between native date inputs (local) and the RFC 3339 wire format.

/** Format an event's date/time (range) for display, honouring the all-day flag. */
export function formatWhen(startsAt: string, endsAt: string | undefined, allDay: boolean): string {
  if (!startsAt) {
    return '';
  }
  const opts: Intl.DateTimeFormatOptions = allDay
    ? { dateStyle: 'medium' }
    : { dateStyle: 'medium', timeStyle: 'short' };
  const fmt = (iso: string) => new Date(iso).toLocaleString(undefined, opts);
  return endsAt ? `${fmt(startsAt)} – ${fmt(endsAt)}` : fmt(startsAt);
}

const pad = (n: number) => String(n).padStart(2, '0');

/** RFC 3339 → a native `datetime-local`/`date` input value (local time). */
export function toInputValue(rfc3339: string, allDay: boolean): string {
  if (!rfc3339) {
    return '';
  }
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) {
    return '';
  }
  const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  return allDay ? date : `${date}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** A native date input value → RFC 3339 (UTC), or `''` when empty/invalid. */
export function toRfc3339(inputValue: string): string {
  if (!inputValue) {
    return '';
  }
  const d = new Date(inputValue);
  return Number.isNaN(d.getTime()) ? '' : d.toISOString();
}
