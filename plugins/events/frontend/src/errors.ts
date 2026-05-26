/**
 * Backend → frontend error-message mapping for the events plugin.
 *
 * Connect-RPC handlers emit stable `events.error.<code>` IDs (with optional
 * positional args separated by `:`) as `ConnectError` messages. This hook
 * decodes the wire form and renders the matching translated string via Lingui.
 *
 * Keeping the mapping in one file means a new BE error code is one branch
 * here + one msgid extracted by `lingui extract`. The hook form is required
 * because the `t` macro must be called inside a component that's inside an
 * `<I18nProvider>`.
 */

import { ConnectError } from '@connectrpc/connect';
import { useLingui } from '@lingui/react/macro';

/** Returns a function that maps an unknown error into a translated string. */
export function useEventsError(): (err: unknown) => string {
  const { t } = useLingui();
  return (err: unknown): string => {
    if (!(err instanceof ConnectError)) {
      return String(err);
    }
    const [code, ...args] = err.rawMessage.split(':');
    const field = args[0] ?? '';
    switch (code) {
      case 'events.error.auth_required':
        return t`Sign in to continue.`;
      case 'events.error.field_invalid':
        return t`Invalid value for ${field}.`;
      case 'events.error.field_required':
        return t`${field} is required.`;
      case 'events.error.field_invalid_rfc3339':
        return t`${field} must be a valid date/time.`;
      case 'events.error.field_invalid_visibility':
        return t`Visibility must be private or public.`;
      case 'events.error.field_invalid_owner_kind':
        return t`Owner must be yourself or a group.`;
      case 'events.error.field_invalid_principal_kind':
        return t`Share target must be a user, group, or "public".`;
      case 'events.error.group_membership_required':
        return t`You must be a member of the owning group.`;
      case 'events.error.group_write_required':
        return t`You need write access in this group.`;
      case 'events.error.invite_already_exists':
        return t`This event already has an invite.`;
      default:
        // Unknown code → render the raw wire string so the developer can see
        // what's coming back. CI's pseudo-locale gate catches missed entries.
        return err.rawMessage;
    }
  };
}
