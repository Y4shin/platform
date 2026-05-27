import { useMutation, useQuery } from '@connectrpc/connect-query';
import { Button, Card, Input, Stack } from '@junius/design';
import { rpc } from '@junius/generated/admin/rpc';
import { useState } from 'react';

import { PermissionPicker } from '../../lib/PermissionPicker.js';

/**
 * The user-role editor (`/p/admin/user-roles`): list every user-role, edit
 * its permission set, manage assignments. The built-in `admin` role can be
 * assigned (so an admin can promote a colleague) but its permissions and
 * existence are immutable — `is_builtin` rows can't be edited or deleted.
 */
export function UserRolesPage() {
  const rolesQ = useQuery(rpc.UserRoleAdminService.listUserRoles, {});
  const assignmentsQ = useQuery(rpc.UserRoleAdminService.listUserRoleAssignments, {});

  const createRole = useMutation(rpc.UserRoleAdminService.createUserRole, {
    onSuccess: () => rolesQ.refetch(),
  });
  const deleteRole = useMutation(rpc.UserRoleAdminService.deleteUserRole, {
    onSuccess: () => rolesQ.refetch(),
  });
  const setPerms = useMutation(rpc.UserRoleAdminService.setUserRolePermissions, {
    onSuccess: () => rolesQ.refetch(),
  });

  const findUser = useMutation(rpc.UserAdminService.findUserByEmail);
  const assign = useMutation(rpc.UserRoleAdminService.assignUserRole, {
    onSuccess: () => assignmentsQ.refetch(),
  });
  const revoke = useMutation(rpc.UserRoleAdminService.revokeUserRole, {
    onSuccess: () => assignmentsQ.refetch(),
  });

  const [newRoleName, setNewRoleName] = useState('');
  const [newRoleDesc, setNewRoleDesc] = useState('');
  const [assignEmail, setAssignEmail] = useState('');
  const [assignRoleId, setAssignRoleId] = useState('');
  const [assignError, setAssignError] = useState<string | null>(null);

  return (
    <Stack gap="md">
      <h1 className="font-semibold text-xl">User roles</h1>
      <p className="text-fg-2 text-sm">
        Global-scope roles. A user holding any role with the wildcard permission <code>*</code>{' '}
        bypasses every ACL check (the built-in <code>admin</code> role works exactly this way).
      </p>

      <Card>
        <h2 className="font-medium text-base">All roles</h2>
        {rolesQ.isPending ? (
          <p className="text-fg-2 text-sm">Loading…</p>
        ) : rolesQ.error ? (
          <p className="text-danger text-sm">{rolesQ.error.message}</p>
        ) : (
          <ul className="flex flex-col gap-3">
            {rolesQ.data.roles.map((r) => (
              <li key={r.id} className="rounded border p-3">
                <div className="mb-2 flex items-center justify-between">
                  <div>
                    <span className="font-medium">{r.name}</span>
                    {r.isBuiltin ? (
                      <span className="ml-2 rounded bg-bg-2 px-2 py-0.5 text-fg-2 text-xs">
                        built-in
                      </span>
                    ) : null}
                    {r.description ? (
                      <div className="text-fg-2 text-sm">{r.description}</div>
                    ) : null}
                  </div>
                  {!r.isBuiltin ? (
                    <Button
                      onClick={() => deleteRole.mutate({ id: r.id })}
                      disabled={deleteRole.isPending}
                    >
                      Delete role
                    </Button>
                  ) : null}
                </div>
                {r.isBuiltin ? (
                  <div className="text-fg-2 text-xs">
                    Permissions: {r.permissions.length === 0 ? '(none)' : r.permissions.join(', ')}
                  </div>
                ) : (
                  <PermissionPicker
                    selected={r.permissions}
                    onChange={(perms) => setPerms.mutate({ roleId: r.id, permissions: perms })}
                    disabled={setPerms.isPending}
                  />
                )}
              </li>
            ))}
          </ul>
        )}

        <form
          className="mt-3 flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!newRoleName.trim()) return;
            createRole.mutate(
              { name: newRoleName.trim(), description: newRoleDesc.trim() },
              {
                onSuccess: () => {
                  setNewRoleName('');
                  setNewRoleDesc('');
                },
              },
            );
          }}
        >
          <h3 className="font-medium text-sm">Create user-role</h3>
          <Input
            value={newRoleName}
            onChange={(e) => setNewRoleName(e.currentTarget.value)}
            placeholder="Role name (e.g. support)"
          />
          <Input
            value={newRoleDesc}
            onChange={(e) => setNewRoleDesc(e.currentTarget.value)}
            placeholder="Description (optional)"
          />
          <Button type="submit" disabled={createRole.isPending || !newRoleName.trim()}>
            Create role
          </Button>
        </form>
      </Card>

      <Card>
        <h2 className="font-medium text-base">Assignments</h2>
        {assignmentsQ.isPending ? (
          <p className="text-fg-2 text-sm">Loading…</p>
        ) : assignmentsQ.error ? (
          <p className="text-danger text-sm">{assignmentsQ.error.message}</p>
        ) : assignmentsQ.data.assignments.length === 0 ? (
          <p className="text-fg-2 text-sm">No user-role assignments yet.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {assignmentsQ.data.assignments.map((a) => (
              <li
                key={`${a.userId}:${a.roleId}`}
                className="flex items-center justify-between rounded border p-2"
              >
                <div>
                  <div className="font-medium">{a.displayName}</div>
                  <div className="text-fg-2 text-sm">
                    {a.email} — {a.roleName}
                  </div>
                </div>
                <Button
                  onClick={() => revoke.mutate({ userId: a.userId, roleId: a.roleId })}
                  disabled={revoke.isPending}
                >
                  Revoke
                </Button>
              </li>
            ))}
          </ul>
        )}

        <form
          className="mt-3 flex flex-col gap-2"
          onSubmit={async (e) => {
            e.preventDefault();
            setAssignError(null);
            if (!assignEmail.trim() || !assignRoleId) return;
            try {
              const found = await findUser.mutateAsync({ email: assignEmail.trim() });
              if (!found.userId) {
                setAssignError('No user with that email. Have they logged in once?');
                return;
              }
              await assign.mutateAsync({ userId: found.userId, roleId: assignRoleId });
              setAssignEmail('');
            } catch (err) {
              setAssignError(err instanceof Error ? err.message : String(err));
            }
          }}
        >
          <h3 className="font-medium text-sm">Assign user-role</h3>
          <Input
            value={assignEmail}
            onChange={(e) => setAssignEmail(e.currentTarget.value)}
            placeholder="user@example.com"
          />
          <select
            className="rounded border p-2"
            value={assignRoleId}
            onChange={(e) => setAssignRoleId(e.currentTarget.value)}
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
              assign.isPending || findUser.isPending || !assignEmail.trim() || !assignRoleId
            }
          >
            Assign
          </Button>
          {assignError ? <p className="text-danger text-sm">{assignError}</p> : null}
        </form>
      </Card>
    </Stack>
  );
}
