import { Card, Stack } from '@junius/design';
import { useUser } from '@junius/sdk';
import { useParams } from '@tanstack/react-router';

/**
 * Public (login-optional) invite page (Stage 3 places the seam; Stage 9 wires
 * it to `InviteService.getInvite`/`signup`). It renders for logged-out visitors
 * — `useUser()` is non-null only when a session happens to be present, used to
 * upgrade owner-only affordances.
 */
export function PublicInvitePage() {
  const { slug } = useParams({ strict: false }) as { slug?: string };
  const user = useUser();
  return (
    <Stack gap="md">
      <Card>
        <Stack gap="sm">
          <h1 className="text-xl font-semibold">Event invite</h1>
          <p className="text-fg-2 text-sm">
            Invite <code>{slug ?? '(none)'}</code>. The sign-up surface lands in a later stage.
          </p>
          <p className="text-fg-2 text-sm">
            {user ? `Signed in as ${user.displayName}.` : 'Viewing as a guest.'}
          </p>
        </Stack>
      </Card>
    </Stack>
  );
}
