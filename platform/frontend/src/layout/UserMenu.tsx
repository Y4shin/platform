import { Menu } from '@junius/design';
import { signOut, useUser } from '@junius/sdk';
import { Trans, useLingui } from '@lingui/react/macro';
import { ChevronDown, LogOut } from 'lucide-react';

/**
 * Header user menu: a dropdown triggered by the signed-in user's display name.
 * Reveals their email, a Profile link to `/me`, and a Sign out action that ends
 * the session and returns to login. Replaces the static display-name `<span>`
 * the M04 shell carried.
 *
 * The Profile target `/me` lands in slice #3; this links to it eagerly with a
 * plain anchor so the menu doesn't depend on a not-yet-registered route.
 */
export function UserMenu() {
  const user = useUser();
  const { t } = useLingui();

  if (!user) {
    return null;
  }

  return (
    <Menu>
      <Menu.Trigger
        aria-label={t`User menu`}
        className="text-fg-2 hover:text-fg-1 inline-flex items-center gap-1 text-sm outline-none"
      >
        {user.displayName}
        <ChevronDown aria-hidden className="h-4 w-4" />
      </Menu.Trigger>
      <Menu.Content>
        <Menu.Label>{user.email}</Menu.Label>
        <Menu.Separator />
        <Menu.Item asChild>
          <a href="/me">
            <Trans>Profile</Trans>
          </a>
        </Menu.Item>
        <Menu.Separator />
        <Menu.Item onSelect={() => void signOut()}>
          <LogOut aria-hidden className="h-4 w-4" />
          <Trans>Sign out</Trans>
        </Menu.Item>
      </Menu.Content>
    </Menu>
  );
}
