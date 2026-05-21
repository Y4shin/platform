import type { HTMLAttributes, ReactNode } from 'react';

import { cx } from './cx.js';

export type StackGap = 'sm' | 'md' | 'lg';
export type StackDirection = 'row' | 'column';

export interface StackProps extends HTMLAttributes<HTMLDivElement> {
  gap?: StackGap;
  direction?: StackDirection;
  children: ReactNode;
}

const GAP_CLASSES: Record<StackGap, string> = {
  sm: 'gap-2',
  md: 'gap-4',
  lg: 'gap-6',
};

const DIRECTION_CLASSES: Record<StackDirection, string> = {
  row: 'flex flex-row',
  column: 'flex flex-col',
};

export function Stack({
  gap = 'md',
  direction = 'column',
  className,
  children,
  ...rest
}: StackProps) {
  return (
    <div className={cx(DIRECTION_CLASSES[direction], GAP_CLASSES[gap], className)} {...rest}>
      {children}
    </div>
  );
}
