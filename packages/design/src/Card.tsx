import type { HTMLAttributes, ReactNode } from 'react';

import { cx } from './cx.js';

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export function Card({ className, children, ...rest }: CardProps) {
  return (
    <div
      className={cx(
        'rounded-lg border border-border bg-surface-1 p-4 text-fg-1 shadow-sm',
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  );
}
