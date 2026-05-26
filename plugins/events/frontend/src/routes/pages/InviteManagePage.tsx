import { useMutation, useQuery } from '@connectrpc/connect-query';
import { Button, Card, Input, Stack } from '@junius/design';
import { rpc } from '@junius/generated/events/rpc';
import { Trans, useLingui } from '@lingui/react/macro';
import { useParams } from '@tanstack/react-router';
import { useEffect, useState } from 'react';

interface InviteConfigState {
  signupEnabled: boolean;
  signupOpen: boolean;
  slotLimit: number; // 0 = unlimited
  showTitle: boolean;
  showDatetime: boolean;
  showLocation: boolean;
  showDescription: boolean;
  showRemaining: boolean;
}

const DEFAULT_CONFIG: InviteConfigState = {
  signupEnabled: true,
  signupOpen: true,
  slotLimit: 0,
  showTitle: true,
  showDatetime: true,
  showLocation: true,
  showDescription: true,
  showRemaining: true,
};

// Toggle labels are translated inside the component (the `t` macro must run
// against the active locale, so it can't live at module scope).
const TOGGLE_KEYS: Array<keyof InviteConfigState> = [
  'signupEnabled',
  'signupOpen',
  'showTitle',
  'showDatetime',
  'showLocation',
  'showDescription',
  'showRemaining',
];

/** Owner-side: configure an event's invite page (create or update) and review its
 * sign-ups. The shareable public link is `/i/events/<slug>`. */
export function InviteManagePage() {
  const { eventId } = useParams({ strict: false }) as { eventId?: string };
  const id = eventId ?? '';
  const enabled = Boolean(eventId);

  const invite = useQuery(rpc.InviteService.getEventInvite, { eventId: id }, { enabled });
  const signups = useQuery(rpc.InviteService.listSignups, { eventId: id }, { enabled });

  const existing = invite.data?.hasInvite ? invite.data.invite : undefined;
  const [config, setConfig] = useState<InviteConfigState>(DEFAULT_CONFIG);
  const [presignupGroup, setPresignupGroup] = useState(false);

  // Sync the form from the loaded invite (runs once per loaded invite id).
  useEffect(() => {
    if (existing) {
      setConfig({
        signupEnabled: existing.signupEnabled,
        signupOpen: existing.signupOpen,
        slotLimit: existing.slotLimit,
        showTitle: existing.showTitle,
        showDatetime: existing.showDatetime,
        showLocation: existing.showLocation,
        showDescription: existing.showDescription,
        showRemaining: existing.showRemaining,
      });
    }
  }, [existing]);

  const onSaved = () => {
    void invite.refetch();
    void signups.refetch();
  };
  const create = useMutation(rpc.InviteService.createInvite, { onSuccess: onSaved });
  const update = useMutation(rpc.InviteService.updateInvite, { onSuccess: onSaved });

  const save = () => {
    if (invite.data?.hasInvite) {
      update.mutate({ eventId: id, ...config });
    } else {
      create.mutate({ eventId: id, ...config, presignupGroup });
    }
  };

  const set = (key: keyof InviteConfigState, value: boolean) =>
    setConfig((c) => ({ ...c, [key]: value }));

  const { t } = useLingui();
  const toggleLabels: Record<keyof InviteConfigState, string> = {
    signupEnabled: t`Collect sign-ups`,
    signupOpen: t`Sign-up open`,
    slotLimit: '', // not a toggle; handled separately below
    showTitle: t`Show title`,
    showDatetime: t`Show date/time`,
    showLocation: t`Show location`,
    showDescription: t`Show description`,
    showRemaining: t`Show remaining slots`,
  };
  const anonymousLabel = t`(anonymous)`;

  return (
    <Stack gap="md">
      <h1 className="font-semibold text-xl">
        <Trans>Invite</Trans>
      </h1>

      {existing && (
        <Card>
          <Stack gap="sm">
            <h2 className="font-semibold text-base">
              <Trans>Shareable link</Trans>
            </h2>
            <code className="text-sm">/i/events/{existing.slug}</code>
          </Stack>
        </Card>
      )}

      <Card>
        <Stack gap="sm">
          <h2 className="font-semibold text-base">
            <Trans>Configuration</Trans>
          </h2>
          {TOGGLE_KEYS.map((key) => (
            <label key={key} className="inline-flex items-center gap-2 text-fg-1 text-sm">
              <input
                type="checkbox"
                checked={config[key] as boolean}
                onChange={(e) => set(key, e.target.checked)}
              />
              {toggleLabels[key]}
            </label>
          ))}
          <label className="text-fg-1 text-sm" htmlFor="slot-limit">
            <Trans>Slot limit (0 = unlimited)</Trans>
          </label>
          <Input
            id="slot-limit"
            type="number"
            min={0}
            value={config.slotLimit}
            onChange={(e) => setConfig((c) => ({ ...c, slotLimit: Number(e.target.value) || 0 }))}
          />
          {!existing && (
            <label className="inline-flex items-center gap-2 text-fg-1 text-sm">
              <input
                type="checkbox"
                checked={presignupGroup}
                onChange={(e) => setPresignupGroup(e.target.checked)}
              />
              <Trans>Pre-sign-up current group members (group events)</Trans>
            </label>
          )}
          <div>
            <Button onClick={save} disabled={create.isPending || update.isPending}>
              {existing ? <Trans>Save invite</Trans> : <Trans>Create invite</Trans>}
            </Button>
          </div>
        </Stack>
      </Card>

      <Card>
        <Stack gap="sm">
          <h2 className="font-semibold text-base">
            <Trans>Sign-ups</Trans>
          </h2>
          {signups.data?.signups.length ? (
            <ul className="text-sm">
              {signups.data.signups.map((s) => (
                <li key={s.id} className={s.status === 'opted_out' ? 'text-fg-2 line-through' : ''}>
                  {s.displayName || s.email || anonymousLabel} — {s.status}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-fg-2 text-sm">
              <Trans>No sign-ups yet.</Trans>
            </p>
          )}
        </Stack>
      </Card>
    </Stack>
  );
}
