import { useQuery } from '@connectrpc/connect-query';
import { rpc } from '@junius/generated/admin/rpc';
import { useMemo, useState } from 'react';

interface PermissionPickerProps {
  selected: readonly string[];
  onChange: (permissions: string[]) => void;
  disabled?: boolean;
}

/**
 * Grouped multi-select over every plugin's declared `[permissions]`. The
 * catalogue is sourced from the host's `PermissionCatalogService`, so the
 * list automatically reflects whatever plugins are enabled in the deployment;
 * `junius check` already validates that every permission key is well-formed
 * and declared. Saving is debounced one tick after the selection settles.
 */
export function PermissionPicker({ selected, onChange, disabled }: PermissionPickerProps) {
  const { data, isPending, error } = useQuery(rpc.PermissionCatalogService.listPermissions, {});
  const selectedSet = useMemo(() => new Set(selected), [selected]);

  // Local "pending" set so the UI is responsive while we wait for the server
  // round-trip. Diff against `selectedSet` on commit so flips are atomic.
  const [pending, setPending] = useState<Set<string> | null>(null);
  const effective = pending ?? selectedSet;

  function toggle(name: string) {
    const next = new Set(effective);
    if (next.has(name)) {
      next.delete(name);
    } else {
      next.add(name);
    }
    setPending(next);
  }

  function commit() {
    if (!pending) return;
    onChange(Array.from(pending).sort());
    setPending(null);
  }

  if (error) return <p className="text-danger text-sm">{error.message}</p>;
  if (isPending) return <p className="text-fg-2 text-sm">Loading permissions…</p>;

  return (
    <div className="flex flex-col gap-2">
      {data.plugins.map((p) => (
        <fieldset key={p.plugin} className="rounded border p-2">
          <legend className="text-fg-2 text-xs">{p.displayName}</legend>
          {p.permissions.length === 0 ? (
            <span className="text-fg-2 text-sm">(no permissions)</span>
          ) : (
            <ul className="flex flex-col gap-1">
              {p.permissions.map((perm) => (
                <li key={perm.name}>
                  <label className="flex items-start gap-2 text-sm">
                    <input
                      type="checkbox"
                      disabled={disabled}
                      checked={effective.has(perm.name)}
                      onChange={() => toggle(perm.name)}
                    />
                    <span>
                      <code className="font-mono text-xs">{perm.name}</code>
                      <span className="block text-fg-2 text-xs">{perm.description}</span>
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )}
        </fieldset>
      ))}
      <div className="flex items-center gap-2">
        <button
          type="button"
          className="rounded border px-3 py-1 text-sm"
          onClick={commit}
          disabled={disabled || pending === null}
        >
          Save permissions
        </button>
        {pending !== null ? <span className="text-fg-2 text-xs">unsaved changes</span> : null}
      </div>
    </div>
  );
}
