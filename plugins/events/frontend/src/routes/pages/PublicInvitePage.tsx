import { useMutation, useQuery } from '@connectrpc/connect-query';
import { Button, Card, Input, Stack } from '@junius/design';
import { rpc } from '@junius/generated/events/rpc';
import { useUser } from '@junius/sdk';
import { useParams } from '@tanstack/react-router';
import { useState } from 'react';

import { formatWhen } from '../../domain.js';

/**
 * The public (login-optional) invite page at `/i/events/<slug>`. Renders for
 * logged-out visitors: a public event's invite is reachable by anyone, a private
 * one 404s unless the (logged-in) viewer has access. `useUser()` upgrades the
 * sign-up affordance — a logged-in visitor signs up as themselves, a guest
 * provides a name + email.
 */
export function PublicInvitePage() {
  const { slug } = useParams({ strict: false }) as { slug?: string };
  const s = slug ?? '';
  const user = useUser();
  const [guestName, setGuestName] = useState('');
  const [guestEmail, setGuestEmail] = useState('');

  const invite = useQuery(rpc.InviteService.getInvite, { slug: s }, { enabled: Boolean(slug) });
  const onChanged = () => void invite.refetch();
  const signup = useMutation(rpc.InviteService.signup, { onSuccess: onChanged });
  const optOut = useMutation(rpc.InviteService.optOut, { onSuccess: onChanged });

  if (invite.isPending) {
    return (
      <Card>
        <p className="text-fg-2 text-sm">Loading…</p>
      </Card>
    );
  }
  if (invite.error || !invite.data) {
    return (
      <Card>
        <p className="text-fg-2 text-sm">This invite is not available.</p>
      </Card>
    );
  }

  const page = invite.data;
  const when = formatWhen(page.startsAt, page.endsAt || undefined, page.allDay);

  return (
    <Stack gap="md">
      <Card>
        <Stack gap="sm">
          <h1 className="font-semibold text-xl">{page.title || 'You’re invited'}</h1>
          {when && <p className="text-fg-2 text-sm">{when}</p>}
          {page.location && <p className="text-fg-2 text-sm">📍 {page.location}</p>}
          {page.description && <p className="text-fg-1 text-sm">{page.description}</p>}
          {page.showRemaining && !page.unlimitedSlots && (
            <p className="text-fg-2 text-sm">{page.slotsRemaining} slot(s) remaining.</p>
          )}
        </Stack>
      </Card>

      {page.canSignup ? (
        <Card>
          <Stack gap="sm">
            <h2 className="font-semibold text-base">Sign up</h2>
            {user ? (
              <p className="text-fg-2 text-sm">Signing up as {user.displayName}.</p>
            ) : (
              <>
                <Input
                  placeholder="Your name"
                  value={guestName}
                  onChange={(e) => setGuestName(e.target.value)}
                />
                <Input
                  placeholder="Your email"
                  type="email"
                  value={guestEmail}
                  onChange={(e) => setGuestEmail(e.target.value)}
                />
              </>
            )}
            <div className="flex gap-2">
              <Button
                disabled={signup.isPending}
                onClick={() =>
                  signup.mutate(user ? { slug: s } : { slug: s, guestName, guestEmail })
                }
              >
                I’m going
              </Button>
              {user && (
                <Button
                  variant="ghost"
                  disabled={optOut.isPending}
                  onClick={() => optOut.mutate({ slug: s })}
                >
                  Opt out
                </Button>
              )}
            </div>
            {signup.error && <p className="text-danger text-sm">{String(signup.error)}</p>}
          </Stack>
        </Card>
      ) : (
        <Card>
          <p className="text-fg-2 text-sm">Sign-up is closed.</p>
        </Card>
      )}
    </Stack>
  );
}
