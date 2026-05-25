import { forwardRef, type InputHTMLAttributes } from 'react';

import { cx } from './cx.js';

/** Shared control styling for text-like inputs (Input/Select/Textarea/DateTime). */
export const CONTROL_CLASS =
  'block w-full rounded-md border border-border bg-surface-1 px-3 py-2 text-sm text-fg-1 ' +
  'placeholder:text-fg-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary ' +
  'disabled:cursor-not-allowed disabled:opacity-50';

export type InputProps = InputHTMLAttributes<HTMLInputElement>;

/** A styled text input. Forwards its ref so react-hook-form can register it. */
export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { className, ...rest },
  ref,
) {
  return <input ref={ref} className={cx(CONTROL_CLASS, className)} {...rest} />;
});
