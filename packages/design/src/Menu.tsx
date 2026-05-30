import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import type { ComponentPropsWithoutRef, ReactNode } from 'react';

import { cx } from './cx.js';

/**
 * Dropdown menu primitive — a thin, styled wrapper over
 * `@radix-ui/react-dropdown-menu` (unstyled + a11y-correct: roving focus, type
 * ahead, `role="menu"`/`menuitem`, Escape-to-close). The host user menu is the
 * first consumer; plugin row-action menus reuse it later.
 *
 * Composition mirrors Radix: `<Menu>` (root) → `<Menu.Trigger>` +
 * `<Menu.Content>` containing `<Menu.Item>` / `<Menu.Label>` / `<Menu.Separator>`.
 * `asChild` is forwarded so a trigger can be your own `<button>` and an item can
 * be a router `<Link>`.
 */
export function Menu({ children }: { children: ReactNode }) {
  return <DropdownMenu.Root>{children}</DropdownMenu.Root>;
}

function Trigger(props: ComponentPropsWithoutRef<typeof DropdownMenu.Trigger>) {
  return <DropdownMenu.Trigger {...props} />;
}

function Content({
  className,
  children,
  sideOffset = 6,
  align = 'end',
  ...rest
}: ComponentPropsWithoutRef<typeof DropdownMenu.Content>) {
  return (
    <DropdownMenu.Portal>
      <DropdownMenu.Content
        sideOffset={sideOffset}
        align={align}
        className={cx(
          'bg-surface-1 border-border z-50 min-w-44 rounded-md border p-1 shadow-md',
          className,
        )}
        {...rest}
      >
        {children}
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  );
}

function Item({ className, ...rest }: ComponentPropsWithoutRef<typeof DropdownMenu.Item>) {
  return (
    <DropdownMenu.Item
      className={cx(
        'text-fg-1 flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-sm outline-none',
        'focus:bg-surface-2 data-[highlighted]:bg-surface-2',
        className,
      )}
      {...rest}
    />
  );
}

function Label({ className, ...rest }: ComponentPropsWithoutRef<typeof DropdownMenu.Label>) {
  return (
    <DropdownMenu.Label className={cx('text-fg-2 px-2 py-1.5 text-xs', className)} {...rest} />
  );
}

function Separator({
  className,
  ...rest
}: ComponentPropsWithoutRef<typeof DropdownMenu.Separator>) {
  return <DropdownMenu.Separator className={cx('bg-border my-1 h-px', className)} {...rest} />;
}

Menu.Trigger = Trigger;
Menu.Content = Content;
Menu.Item = Item;
Menu.Label = Label;
Menu.Separator = Separator;
