import { Slot } from '@radix-ui/react-slot';
import type { ButtonHTMLAttributes, ReactNode } from 'react';

import { cx } from './cx.js';

export type ButtonVariant = 'primary' | 'secondary' | 'ghost';
export type ButtonSize = 'sm' | 'md' | 'lg';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Render as a child element instead of a `<button>` (Radix Slot pattern). */
  asChild?: boolean;
  children: ReactNode;
}

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  primary:
    'bg-primary text-primary-fg hover:opacity-90 focus-visible:ring-2 focus-visible:ring-primary',
  secondary:
    'bg-surface-2 text-fg-1 hover:bg-surface-1 focus-visible:ring-2 focus-visible:ring-fg-2',
  ghost: 'bg-transparent text-fg-1 hover:bg-surface-2',
};

const SIZE_CLASSES: Record<ButtonSize, string> = {
  sm: 'h-7 px-2 text-sm',
  md: 'h-9 px-3 text-sm',
  lg: 'h-11 px-4 text-base',
};

const BASE =
  'inline-flex items-center justify-center gap-2 rounded-md font-medium ' +
  'transition-colors focus-visible:outline-none ' +
  'disabled:pointer-events-none disabled:opacity-50';

export function Button({
  variant = 'primary',
  size = 'md',
  asChild,
  className,
  children,
  ...rest
}: ButtonProps) {
  const Comp = asChild ? Slot : 'button';
  return (
    <Comp className={cx(BASE, VARIANT_CLASSES[variant], SIZE_CLASSES[size], className)} {...rest}>
      {children}
    </Comp>
  );
}
