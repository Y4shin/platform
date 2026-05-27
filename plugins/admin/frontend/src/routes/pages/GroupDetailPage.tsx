import { useMutation, useQuery } from '@connectrpc/connect-query';
import { Button, Card, Input, Stack } from '@junius/design';
import { rpc } from '@junius/generated/admin/rpc';
import { useParams } from '@tanstack/react-router';
import { useState } from 'react';

import { PermissionPicker } from '../../lib/PermissionPicker.js';

/**
 * Per-group editor: list + create + delete group-roles (each with its own
 * permission set via `PermissionPicker`), list + add + remove members.
 * `managed_by` provenance is shown per row; the M18 plan's lock_managed
 * (Stage 4) makes config/oidc rows server-side immutable, so the Remove
 * button is also disabled here for non-manual rows.
 */
export function GroupDetailPage() {
  const { groupId } = useParams({ strict: false }) as { groupId: string };

  const groupsQ = useQuery(rpc.GroupAdminService.listGroups, {});
  const rolesQ = useQuery(rpc.GroupAdminService.listGroupRoles, { groupId });
  const membersQ = useQuery(rpc.GroupAdminService.listGroupMembers, { groupId });

  const group = groupsQ.data?.groups.find((g) => g.id === groupId);

  const createRole = useMutation(rpc.GroupAdminService.createGroupRole, {
    onSuccess: () => rolesQ.refetch(),
  });
  const setRolePerms = useMutation(rpc.GroupAdminService.setGroupRolePermissions, {
    onSuccess: () => rolesQ.refetch(),
  });
  const deleteRole = useMutation(rpc.GroupAdminService.deleteGroupRole, {
    onSuccess: () => rolesQ.refetch(),
  });

  const findUser = useMutation(rpc.UserAdminService.findUserByEmail);
  const addMember = useMutation(rpc.GroupAdminService.addGroupMember, {
    onSuccess: () => membersQ.refetch(),
  });
  const removeMember = useMutation(rpc.GroupAdminService.removeGroupMember, {
    onSuccess: () => membersQ.refetch(),
  });

  const [newRoleName, setNewRoleName] = useState('');
  const [memberEmail, setMemberEmail] = useState('');
  const [memberRoleId, setMemberRoleId] = useState('');
  const [memberError, setMemberError] = useState<string | null>(null);

  return (
    <Stack gap="md">
      <h1 className="font-semibold text-xl">{group?.name ?? 'Group'}</h1>
      {group?.description ? <p className="text-fg-2 text-sm">{group.description}</p> : null}

      <Card>
        <h2 className="font-medium text-base">Roles</h2>
        {rolesQ.isPending ? (
          <p className="text-fg-2 text-sm">Loading…</p>
        ) : rolesQ.error ? (
          <p className="text-danger text-sm">{rolesQ.error.message}</p>
        ) : (
          <ul className="flex flex-col gap-3">
            {rolesQ.data.roles.map((r) => (
              <li key={r.id} className="rounded border p-3">
                <div className="mb-2 flex items-center justify-between">
                  <div className="font-medium">{r.name}</div>
                  <Button
                    onClick={() => deleteRole.mutate({ roleId: r.id })}
                    disabled={deleteRole.isPending}
                  >
                    Delete role
                  </Button>
                </div>
                <PermissionPicker
                  selected={r.permissions}
                  onChange={(perms) => setRolePerms.mutate({ roleId: r.id, permissions: perms })}
                  disabled={setRolePerms.isPending}
                />
              </li>
            ))}
          </ul>
        )}

        <form
          className="mt-3 flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!newRoleName.trim()) return;
            createRole.mutate(
              { groupId, name: newRoleName.trim() },
              { onSuccess: () => setNewRoleName('') },
            );
          }}
        >
          <Input
            value={newRoleName}
            onChange={(e) => setNewRoleName(e.currentTarget.value)}
            placeholder="New role name"
          />
          <Button type="submit" disabled={createRole.isPending || !newRoleName.trim()}>
            Add role
          </Button>
        </form>
      </Card>

      <Card>
        <h2 className="font-medium text-base">Members</h2>
        {membersQ.isPending ? (
          <p className="text-fg-2 text-sm">Loading…</p>
        ) : membersQ.error ? (
          <p className="text-danger text-sm">{membersQ.error.message}</p>
        ) : membersQ.data.members.length === 0 ? (
          <p className="text-fg-2 text-sm">No members yet.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {membersQ.data.members.map((m) => (
              <li key={m.userId} className="flex items-center justify-between rounded border p-2">
                <div>
                  <div className="font-medium">{m.displayName}</div>
                  <div className="text-fg-2 text-sm">
                    {m.email} — {m.roleName}
                  </div>
                  {m.managedBy !== 'manual' ? (
                    <div className="text-fg-2 text-xs">
                      Managed by {m.managedBy}
                      {m.managedSource ? ` (${m.managedSource})` : ''}
                    </div>
                  ) : null}
                </div>
                <Button
                  onClick={() => removeMember.mutate({ groupId, userId: m.userId })}
                  disabled={m.managedBy !== 'manual' || removeMember.isPending}
                >
                  Remove
                </Button>
              </li>
            ))}
          </ul>
        )}

        <form
          className="mt-3 flex flex-col gap-2"
          onSubmit={async (e) => {
            e.preventDefault();
            setMemberError(null);
            if (!memberEmail.trim() || !memberRoleId) return;
            try {
              const found = await findUser.mutateAsync({ email: memberEmail.trim() });
              if (!found.userId) {
                setMemberError('No user with that email. Have they logged in once?');
                return;
              }
              await addMember.mutateAsync({
                groupId,
                userId: found.userId,
                roleId: memberRoleId,
              });
              setMemberEmail('');
            } catch (err) {
              setMemberError(err instanceof Error ? err.message : String(err));
            }
          }}
        >
          <h3 className="font-medium text-sm">Add member</h3>
          <Input
            value={memberEmail}
            onChange={(e) => setMemberEmail(e.currentTarget.value)}
            placeholder="user@example.com"
          />
          <select
            className="rounded border p-2"
            value={memberRoleId}
            onChange={(e) => setMemberRoleId(e.currentTarget.value)}
          >
            <option value="">Choose a role…</option>
            {rolesQ.data?.roles.map((r) => (
              <option key={r.id} value={r.id}>
                {r.name}
              </option>
            ))}
          </select>
          <Button
            type="submit"
            disabled={
              addMember.isPending || findUser.isPending || !memberEmail.trim() || !memberRoleId
            }
          >
            Add
          </Button>
          {memberError ? <p className="text-danger text-sm">{memberError}</p> : null}
        </form>
      </Card>
    </Stack>
  );
}
