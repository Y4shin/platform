import { useMutation, useQuery } from '@connectrpc/connect-query';
import { Button, Card, Input, Stack } from '@junius/design';
import { rpc } from '@junius/generated/admin/rpc';
import { usePluginNavigate } from '@junius/sdk';
import { useState } from 'react';

/**
 * The admin's landing page (mounted at `/p/admin`): list every group plus an
 * inline form to create a new one. Click-through to `/p/admin/groups/$id` for
 * the per-group editor.
 */
export function GroupsListPage() {
  const nav = usePluginNavigate();
  const list = useQuery(rpc.GroupAdminService.listGroups, {});
  const create = useMutation(rpc.GroupAdminService.createGroup, {
    onSuccess: () => list.refetch(),
  });

  const [name, setName] = useState('');
  const [description, setDescription] = useState('');

  return (
    <Stack gap="md">
      <h1 className="font-semibold text-xl">Groups</h1>

      <Card>
        <form
          className="flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!name.trim()) return;
            create.mutate(
              { name: name.trim(), description: description.trim() },
              {
                onSuccess: () => {
                  setName('');
                  setDescription('');
                },
              },
            );
          }}
        >
          <h2 className="font-medium text-base">Create group</h2>
          <Input
            value={name}
            onChange={(e) => setName(e.currentTarget.value)}
            placeholder="Group name"
          />
          <Input
            value={description}
            onChange={(e) => setDescription(e.currentTarget.value)}
            placeholder="Description (optional)"
          />
          <Button type="submit" disabled={create.isPending || !name.trim()}>
            {create.isPending ? 'Creating…' : 'Create'}
          </Button>
          {create.error ? <p className="text-danger text-sm">{create.error.message}</p> : null}
        </form>
      </Card>

      <Card>
        <h2 className="font-medium text-base">All groups</h2>
        {list.error ? (
          <p className="text-danger text-sm">{list.error.message}</p>
        ) : list.isPending ? (
          <p className="text-fg-2 text-sm">Loading…</p>
        ) : list.data.groups.length === 0 ? (
          <p className="text-fg-2 text-sm">No groups yet.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {list.data.groups.map((g) => (
              <li key={g.id}>
                <button
                  type="button"
                  className="w-full rounded border p-2 text-left hover:bg-bg-2"
                  onClick={() => nav('/p/admin/groups/$groupId', { groupId: g.id })}
                >
                  <div className="font-medium">{g.name}</div>
                  {g.description ? <div className="text-fg-2 text-sm">{g.description}</div> : null}
                </button>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card>
        <p className="text-sm">
          <button
            type="button"
            className="text-link underline"
            onClick={() => nav('/p/admin/user-roles')}
          >
            Manage user-roles →
          </button>
        </p>
      </Card>
    </Stack>
  );
}
