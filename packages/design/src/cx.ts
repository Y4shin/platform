/**
 * Tiny class-name concatenator. Skips falsy values so callers can write
 * `cx(BASE, isDisabled && DISABLED_CLASS)`. Avoids a `clsx` dep — the
 * design system needs nothing beyond this.
 */
export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter((p): p is string => typeof p === 'string' && p.length > 0).join(' ');
}
