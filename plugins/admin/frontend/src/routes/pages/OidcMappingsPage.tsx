import { useMutation, useQuery } from '@connectrpc/connect-query';
import { Button, Card, Input, Stack } from '@junius/design';
import { rpc } from '@junius/generated/admin/rpc';
import { useState } from 'react';

/**
 * OIDC group → Junius (group, role) mapping editor (`/p/admin/oidc`).
 * Every login reconciles `managed_by='oidc'` group memberships against
 * the user's `groups` claim. Memberships not listed in the claim get
 * reaped on next login; manual/config memberships are untouched.
 */
export function OidcMappingsPage() {
  const mappingsQ = useQuery(rpc.OidcMappingAdminService.listOidcMappings, {});
  const groupsQ = useQuery(rpc.GroupAdminService.listGroups, {});

  const create = useMutation(rpc.OidcMappingAdminService.createOidcMapping, {
    onSuccess: () => mappingsQ.refetch(),
  });
  const del = useMutation(rpc.OidcMappingAdminService.deleteOidcMapping, {
    onSuccess: () => mappingsQ.refetch(),
  });

  const [oidcGroupName, setOidcGroupName] = useState('');
  const [groupId, setGroupId] = useState('');
  const [roleId, setRoleId] = useState('');

  // Per-group role list driven by the chosen group.
  const rolesQ = useQuery(
    rpc.GroupAdminService.listGroupRoles,
    { groupId },
    { enabled: Boolean(groupId) },
  );

  return (
    <Stack gap="md">
      <h1 className="font-semibold text-xl">OIDC group mappings</h1>
      <p className="text-fg-2 text-sm">
        Each row maps an OIDC <code>groups</code>-claim entry to a Junius (group, role) pair.
        Reconciliation runs on every login and on <code>POST /api/me/refresh-groups</code>; the
        resulting memberships carry <code>managed_by=&quot;oidc&quot;</code>.
      </p>

      <Card>
        <h2 className="font-medium text-base">All mappings</h2>
        {mappingsQ.isPending ? (
          <p className="text-fg-2 text-sm">Loading…</p>
        ) : mappingsQ.error ? (
          <p className="text-danger text-sm">{mappingsQ.error.message}</p>
        ) : mappingsQ.data.mappings.length === 0 ? (
          <p className="text-fg-2 text-sm">No mappings yet.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {mappingsQ.data.mappings.map((m) => (
              <li key={m.id} className="flex items-center justify-between rounded border p-2">
                <div>
                  <div className="font-medium">
                    <code>{m.oidcGroupName}</code>
                  </div>
                  <div className="text-fg-2 text-sm">
                    → {m.groupName} / {m.roleName}
                  </div>
                </div>
                <Button onClick={() => del.mutate({ id: m.id })} disabled={del.isPending}>
                  Delete
                </Button>
              </li>
            ))}
          </ul>
        )}

        <form
          className="mt-3 flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!oidcGroupName.trim() || !groupId || !roleId) return;
            create.mutate(
              { oidcGroupName: oidcGroupName.trim(), groupId, roleId },
              {
                onSuccess: () => {
                  setOidcGroupName('');
                  setGroupId('');
                  setRoleId('');
                },
              },
            );
          }}
        >
          <h3 className="font-medium text-sm">Add mapping</h3>
          <Input
            value={oidcGroupName}
            onChange={(e) => setOidcGroupName(e.currentTarget.value)}
            placeholder="OIDC group name (e.g. junius-organisers)"
          />
          <select
            className="rounded border p-2"
            value={groupId}
            onChange={(e) => {
              setGroupId(e.currentTarget.value);
              setRoleId('');
            }}
          >
            <option value="">Choose target group…</option>
            {groupsQ.data?.groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
          <select
            className="rounded border p-2"
            value={roleId}
            onChange={(e) => setRoleId(e.currentTarget.value)}
            disabled={!groupId}
          >
            <option value="">Choose role in that group…</option>
            {rolesQ.data?.roles.map((r) => (
              <option key={r.id} value={r.id}>
                {r.name}
              </option>
            ))}
          </select>
          <Button
            type="submit"
            disabled={create.isPending || !oidcGroupName.trim() || !groupId || !roleId}
          >
            Add mapping
          </Button>
          {create.error ? <p className="text-danger text-sm">{create.error.message}</p> : null}
        </form>
      </Card>
    </Stack>
  );
}
