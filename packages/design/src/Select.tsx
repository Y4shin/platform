import { forwardRef, type SelectHTMLAttributes } from 'react';

import { cx } from './cx.js';
import { CONTROL_CLASS } from './Input.js';

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  options: SelectOption[];
  /** An optional, disabled leading option (e.g. "Choose…"). */
  placeholder?: string;
}

/** A styled `<select>` driven by an `options` list. */
export const Select = forwardRef<HTMLSelectElement, SelectProps>(function Select(
  { className, options, placeholder, ...rest },
  ref,
) {
  return (
    <select ref={ref} className={cx(CONTROL_CLASS, className)} {...rest}>
      {placeholder !== undefined && (
        <option value="" disabled>
          {placeholder}
        </option>
      )}
      {options.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  );
});
