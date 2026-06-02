import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';

import { correlationIdOf } from './RouteErrorBoundary.js';

// The error boundary's correlation id is the trace id the host stamps onto
// error responses as `x-correlation-id` (Connect-Web exposes it as
// `ConnectError.metadata`). This proves the extraction the boundary relies on
// for AC3 ("a correlation id that matches the juniusd log").
describe('correlationIdOf', () => {
  it('pulls x-correlation-id from a ConnectError`s metadata', () => {
    const error = new ConnectError(
      'boom',
      Code.Internal,
      new Headers({ 'x-correlation-id': 'abc123def456' }),
    );
    expect(correlationIdOf(error)).toBe('abc123def456');
  });

  it('returns null for a plain render error with no correlation metadata', () => {
    expect(correlationIdOf(new Error('render blew up'))).toBeNull();
  });

  it('returns null when the metadata header is absent', () => {
    expect(correlationIdOf(new ConnectError('denied', Code.PermissionDenied))).toBeNull();
  });
});
