import { forwardRef, type TextareaHTMLAttributes } from 'react';

import { cx } from './cx.js';
import { CONTROL_CLASS } from './Input.js';

export type TextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement>;

/** A styled multi-line text input. */
export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(function Textarea(
  { className, rows = 4, ...rest },
  ref,
) {
  return <textarea ref={ref} rows={rows} className={cx(CONTROL_CLASS, className)} {...rest} />;
});
