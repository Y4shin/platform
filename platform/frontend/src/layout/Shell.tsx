import { Stack } from '@junius/design';
import { useUser } from '@junius/sdk';
import type { ReactNode } from 'react';

import { NavLinks } from './NavLinks.js';

export interface ShellProps {
  children: ReactNode;
}

export function Shell({ children }: ShellProps) {
  const user = useUser();
  return (
    <div className="flex min-h-full flex-col">
      <header className="border-border bg-surface-1 border-b px-4 py-3">
        <Stack direction="row" gap="md" className="items-center justify-between">
          <strong className="text-base">Junius</strong>
          <NavLinks />
          <span className="text-fg-2 text-sm">{user?.displayName ?? 'Anonymous'}</span>
        </Stack>
      </header>
      <main className="mx-auto w-full max-w-3xl flex-1 p-6">{children}</main>
    </div>
  );
}
