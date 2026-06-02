# Calling the backend

Your proto services become a typed TypeScript client. Pages call them through
Connect-Query hooks, so data fetching, caching and mutation all flow through
TanStack Query with full type inference from the proto.

## The generated namespace

When `junius sync` (or `buf generate`) runs, your plugin's services are emitted
to `packages/generated/` and re-exported under a narrow per-plugin namespace:

```ts
import { rpc } from '@junius/generated/events/rpc';
```

`rpc.EventService.listEvents`, `rpc.EventService.createEvent`, etc. are
fully-typed method descriptors — request and response types come straight from
the proto.

## Queries

Reads use `useQuery` from `@connectrpc/connect-query`:

```tsx
import { useQuery } from '@connectrpc/connect-query';
import { rpc } from '@junius/generated/events/rpc';

export function EventsListPage() {
  const { data, isLoading } = useQuery(rpc.EventService.listEvents, {});

  if (isLoading) return <Spinner />;
  return (
    <ul>
      {data?.events.map((e) => (
        <li key={e.id}>{e.title}</li>
      ))}
    </ul>
  );
}
```

The request message is the second argument (`{}` for an empty request). `data`
is typed as the response message — including server-computed fields like
`viewerCanEdit`.

## Mutations

Writes use `useMutation`, and typically invalidate the relevant query on
success:

```tsx
import { useMutation, useQueryClient } from '@connectrpc/connect-query';
import { rpc } from '@junius/generated/events/rpc';

const qc = useQueryClient();
const create = useMutation(rpc.EventService.createEvent, {
  onSuccess: () => qc.invalidateQueries(/* the listEvents key */),
});

create.mutate({ title, visibility, ownerKind: 'user' });
```

## Authentication is automatic

You don't attach tokens or headers. The transport carries the session cookie;
the host's pass-through session middleware resolves the caller. In split-SSR
deployments the inbound `Cookie` header is forwarded transparently, so the same
`useQuery` call runs as the requesting user on the server too — no plugin-side
wiring (see [Server-side rendering](../quality/ssr.md)).

## Errors come back as codes, not English

Backend handlers return stable error **codes** (e.g.
`events.error.field_required:title`), never English prose — so error messages go
through the i18n catalog. The frontend has a small mapping hook that turns a code
into a localized string:

```tsx
import { useEventsError } from '../errors';

const toMessage = useEventsError();
if (create.error) return <Alert>{toMessage(create.error)}</Alert>;
```

The pattern (a `mod err` on the backend, a `useEventsError` hook on the
frontend) is covered in [Internationalization](../capabilities/i18n.md). For now
the rule is: **don't display a raw error string** — map the code.

## Reading the caller

Inside the authed shell, the current user is available via `useUser()` from
`@junius/sdk`. On a public page it may be `null`; handle both.

Next: building forms with the design system.
