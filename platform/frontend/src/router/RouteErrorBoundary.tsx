import { ConnectError } from '@connectrpc/connect';
import { Card, Stack } from '@junius/design';
import { Trans } from '@lingui/react/macro';
import type { ErrorComponentProps } from '@tanstack/react-router';

// The correlation id is the OTel trace id the host stamps onto error responses
// as `x-correlation-id` (see platform/src/correlation.rs); Connect-Web surfaces
// it as `ConnectError.metadata`. A render error with no originating RPC carries
// none — we then show generic copy rather than a blank id.
export function correlationIdOf(error: unknown): string | null {
  const value = ConnectError.from(error).metadata.get('x-correlation-id');
  return value && value.length > 0 ? value : null;
}

// Wraps the authed layout's `<Outlet />`. In dev it shows the stack to speed up
// debugging; in prod it shows a friendly card plus the correlation id (when the
// error came from an RPC) so the user can quote it and an operator can grep the
// matching `juniusd` log line. The host already logs the 5xx with that id.
export function RouteErrorBoundary({ error }: ErrorComponentProps) {
  const correlationId = correlationIdOf(error);

  if (import.meta.env.DEV) {
    return (
      <Card>
        <Stack gap="sm">
          <h2 className="font-semibold text-lg text-danger">
            <Trans>Something went wrong</Trans>
          </h2>
          <pre className="overflow-auto whitespace-pre-wrap text-fg-2 text-xs">
            {error instanceof Error ? (error.stack ?? error.message) : String(error)}
          </pre>
        </Stack>
      </Card>
    );
  }

  return (
    <Card>
      <Stack gap="sm">
        <h2 className="font-semibold text-lg">
          <Trans>Something went wrong</Trans>
        </h2>
        <p className="text-fg-2 text-sm">
          <Trans>
            An unexpected error occurred. Please try again; if it keeps happening, contact your
            administrator.
          </Trans>
        </p>
        {correlationId ? (
          <p className="text-fg-2 text-xs">
            <Trans>Reference id:</Trans> <code>{correlationId}</code>
          </p>
        ) : null}
      </Stack>
    </Card>
  );
}
