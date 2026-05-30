import { Stack } from '@junius/design';
import type { ReactNode } from 'react';

import { NavLinks } from './NavLinks.js';
import { UserMenu } from './UserMenu.js';

export interface ShellProps {
  children: ReactNode;
}

export function Shell({ children }: ShellProps) {
  return (
    <div className="flex min-h-full flex-col">
      <header className="border-border bg-surface-1 border-b px-4 py-3">
        <Stack direction="row" gap="md" className="items-center justify-between">
          {/* The brand name is intentionally not translated. */}
          <strong className="text-base">Junius</strong>
          <NavLinks />
          {/* The LocaleSwitcher re-homes onto /me in slice #3; the header is now
              brand + nav + user menu. */}
          <UserMenu />
        </Stack>
      </header>
      <main className="mx-auto w-full max-w-3xl flex-1 p-6">{children}</main>
    </div>
  );
}
