import { Button, Card, Stack } from '@junius/design';
import { Trans, useLingui } from '@lingui/react/macro';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { ReactNode } from 'react';

import { LocaleSwitcher } from '../layout/LocaleSwitcher.js';

// The /me payload is the host's `MeResponse` (see platform/src/auth/me.rs):
// the authenticated user flattened, plus the bound OIDC subject and the
// caller's live sessions. The page fetches it directly (not via the cached
// AuthProvider user) so the sessions list reflects revokes immediately.
interface MembershipRow {
  groupId: string;
  groupName: string;
  role: { id: string; name: string };
  permissions: string[];
  managedBy: string;
}

interface UserRoleRow {
  roleId: string;
  roleName: string;
  permissions: string[];
}

interface SessionRow {
  id: string;
  userAgent: string | null;
  lastSeen: string;
  current: boolean;
}

interface MeResponse {
  id: string;
  email: string;
  displayName: string;
  locale: string | null;
  oidcSub: string;
  memberships: MembershipRow[];
  userRoles: UserRoleRow[];
  sessions: SessionRow[];
}

const ME_QUERY_KEY = ['me'] as const;

async function fetchMe(): Promise<MeResponse> {
  const res = await fetch('/api/me', { credentials: 'include' });
  if (!res.ok) {
    throw new Error(`GET /api/me failed: ${res.status}`);
  }
  return (await res.json()) as MeResponse;
}

async function revokeSession(id: string): Promise<void> {
  const res = await fetch(`/api/sessions/${id}`, { method: 'DELETE', credentials: 'include' });
  if (!res.ok) {
    throw new Error(`DELETE /api/sessions/${id} failed: ${res.status}`);
  }
}

export function MePage() {
  const { data, isPending, isError } = useQuery({
    queryKey: ME_QUERY_KEY,
    queryFn: fetchMe,
  });
  const queryClient = useQueryClient();
  const revoke = useMutation({
    mutationFn: revokeSession,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ME_QUERY_KEY }),
  });

  if (isPending) {
    return (
      <p className="text-fg-2">
        <Trans>Loading…</Trans>
      </p>
    );
  }
  if (isError || !data) {
    return (
      <p className="text-danger">
        <Trans>Could not load your profile.</Trans>
      </p>
    );
  }

  return (
    <Stack gap="lg">
      <h1 className="font-semibold text-xl">
        <Trans>Profile</Trans>
      </h1>

      <IdentitySection user={data} />
      <PreferencesSection />
      <MembershipsSection memberships={data.memberships} />
      <UserRolesSection userRoles={data.userRoles} />
      <SessionsSection
        sessions={data.sessions}
        onRevoke={(id) => revoke.mutate(id)}
        revoking={revoke.isPending}
      />
    </Stack>
  );
}

function Section({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <Card>
      <Stack gap="sm">
        <h2 className="font-semibold text-fg-1 text-lg">{title}</h2>
        {children}
      </Stack>
    </Card>
  );
}

function Field({ label, value }: { label: ReactNode; value: ReactNode }) {
  return (
    <div className="flex gap-4">
      <span className="w-40 shrink-0 text-fg-2 text-sm">{label}</span>
      <span className="text-fg-1 text-sm">{value}</span>
    </div>
  );
}

function IdentitySection({ user }: { user: MeResponse }) {
  return (
    <Section title={<Trans>Identity</Trans>}>
      <Field label={<Trans>Display name</Trans>} value={user.displayName} />
      <Field label={<Trans>Email</Trans>} value={user.email} />
      <Field
        label={<Trans>OIDC subject</Trans>}
        value={<span className="font-mono">{user.oidcSub}</span>}
      />
    </Section>
  );
}

function PreferencesSection() {
  return (
    <Section title={<Trans>Preferences</Trans>}>
      <Field label={<Trans>Language</Trans>} value={<LocaleSwitcher />} />
    </Section>
  );
}

function ManagedByBadge({ managedBy }: { managedBy: string }) {
  const label =
    managedBy === 'oidc' ? (
      <Trans>OIDC</Trans>
    ) : managedBy === 'config' ? (
      <Trans>Config</Trans>
    ) : (
      <Trans>Manual</Trans>
    );
  return <span className="rounded-full bg-surface-2 px-2 py-0.5 text-fg-2 text-xs">{label}</span>;
}

function MembershipsSection({ memberships }: { memberships: MembershipRow[] }) {
  const { t } = useLingui();
  return (
    <Section title={<Trans>Group memberships</Trans>}>
      {memberships.length === 0 ? (
        <p className="text-fg-2 text-sm">
          <Trans>You are not a member of any group.</Trans>
        </p>
      ) : (
        <table className="w-full text-left text-sm">
          <thead>
            <tr className="text-fg-2">
              <th className="py-1 pr-4 font-medium">
                <Trans>Group</Trans>
              </th>
              <th className="py-1 pr-4 font-medium">
                <Trans>Role</Trans>
              </th>
              <th className="py-1 pr-4 font-medium">
                <Trans>Permissions</Trans>
              </th>
              <th className="py-1 font-medium">
                <Trans>Managed by</Trans>
              </th>
            </tr>
          </thead>
          <tbody>
            {memberships.map((m) => (
              <tr key={m.groupId} className="border-border border-t">
                <td className="py-1 pr-4 text-fg-1">{m.groupName}</td>
                <td className="py-1 pr-4 text-fg-1">{m.role.name}</td>
                <td className="py-1 pr-4 text-fg-1">{t`${m.permissions.length} permissions`}</td>
                <td className="py-1">
                  <ManagedByBadge managedBy={m.managedBy} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Section>
  );
}

function UserRolesSection({ userRoles }: { userRoles: UserRoleRow[] }) {
  return (
    <Section title={<Trans>User-roles</Trans>}>
      {userRoles.length === 0 ? (
        <p className="text-fg-2 text-sm">
          <Trans>You hold no user-roles.</Trans>
        </p>
      ) : (
        <table className="w-full text-left text-sm">
          <thead>
            <tr className="text-fg-2">
              <th className="py-1 pr-4 font-medium">
                <Trans>Role</Trans>
              </th>
              <th className="py-1 font-medium">
                <Trans>Permissions</Trans>
              </th>
            </tr>
          </thead>
          <tbody>
            {userRoles.map((r) => (
              <tr key={r.roleId} className="border-border border-t">
                <td className="py-1 pr-4 text-fg-1">{r.roleName}</td>
                <td className="py-1 text-fg-1 font-mono">{r.permissions.join(', ')}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Section>
  );
}

function SessionsSection({
  sessions,
  onRevoke,
  revoking,
}: {
  sessions: SessionRow[];
  onRevoke: (id: string) => void;
  revoking: boolean;
}) {
  const { t } = useLingui();
  return (
    <Section title={<Trans>Sessions</Trans>}>
      <table className="w-full text-left text-sm">
        <thead>
          <tr className="text-fg-2">
            <th className="py-1 pr-4 font-medium">
              <Trans>Browser</Trans>
            </th>
            <th className="py-1 pr-4 font-medium">
              <Trans>Last seen</Trans>
            </th>
            <th className="py-1 font-medium" />
          </tr>
        </thead>
        <tbody>
          {sessions.map((s) => (
            <tr key={s.id} className="border-border border-t">
              <td className="py-1 pr-4 text-fg-1">
                {s.userAgent ?? t`Unknown device`}
                {s.current ? (
                  <span className="ml-2 rounded-full bg-surface-2 px-2 py-0.5 text-fg-2 text-xs">
                    <Trans>This device</Trans>
                  </span>
                ) : null}
              </td>
              <td className="py-1 pr-4 text-fg-2">{new Date(s.lastSeen).toLocaleString()}</td>
              <td className="py-1">
                {s.current ? null : (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={revoking}
                    onClick={() => onRevoke(s.id)}
                  >
                    <Trans>Revoke</Trans>
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Section>
  );
}
