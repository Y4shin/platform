# Routes and pages

A plugin's frontend is a package — `@junius/plugin-<name>`, under
`plugins/<name>/frontend/` — that the host shell composes into its router. You
contribute routes by exporting builder functions; `junius sync` mounts them.

## Exporting routes

Your `frontend/src/index.ts` (often re-exporting from `src/routes/index.ts`)
exports `buildRoutes(parent)`, and — for a public surface —
`buildPublicRoutes(parent)`:

```ts
import { type AnyRoute, createRoute } from '@tanstack/react-router';
import { EventsListPage } from './routes/pages/EventsListPage';
import { EventDetailPage } from './routes/pages/EventDetailPage';
import { EventEditPage } from './routes/pages/EventEditPage';

export function buildRoutes(parent: AnyRoute): AnyRoute[] {
  const getParentRoute = () => parent;
  return [
    createRoute({ getParentRoute, path: '/', component: EventsListPage }),
    createRoute({ getParentRoute, path: '/new', component: EventEditPage, beforeLoad: requireWrite }),
    createRoute({ getParentRoute, path: '/$eventId', component: EventDetailPage }),
    createRoute({ getParentRoute, path: '/$eventId/edit', component: EventEditPage, beforeLoad: requireWrite }),
  ];
}

export function buildPublicRoutes(parent: AnyRoute): AnyRoute[] {
  return [
    createRoute({ getParentRoute: () => parent, path: '/$slug', component: PublicInvitePage }),
  ];
}
```

## Where they mount

`junius sync` wires the builders into the generated host route tree:

| Export | Mounted at | Context |
| --- | --- | --- |
| `buildRoutes` | `/p/<name>` | inside the authed shell — user is always present |
| `buildPublicRoutes` | `/i/<name>` | outside the shell — visitor may be logged out |

`buildPublicRoutes` is only mounted if your manifest sets `public_routes = true`
(see [Public and unauthenticated surfaces](../backend/public-surfaces.md)). On a
public page, `useUser()` is non-null only when a session happens to be present —
write the page to render for an anonymous visitor too.

## Navigating to your own sub-routes

The composed route tree **erases plugin route types** — `buildRoutes` returns
`AnyRoute`, so a typed `<Link to="/p/events/$eventId">` won't compile against the
host router's narrowing. Use `usePluginNavigate` / `PluginLink` from
`@junius/sdk`, which take a raw path string + a params record:

```ts
import { PluginLink, usePluginNavigate } from '@junius/sdk';

const nav = usePluginNavigate();
nav('/p/events/$eventId', { eventId: id });

// or, declaratively:
<PluginLink to="/p/events/$eventId" params={{ eventId: id }}>view</PluginLink>
```

These are for navigation **within your own plugin only**. Never use them to jump
into another plugin — the sanctioned cross-plugin surface is
`[exposes.components]` + `useComponent` (see
[Exposed components](./exposed-components.md)).

## Don't gate UI on client-side permissions

`requirePermissions` is a no-op stub today. Gate edit/delete affordances on the
**server-computed `viewerCanEdit` / `viewerCanShare`** flags your RPC responses
carry (see [Ownership, sharing and visibility](../backend/ownership.md)):

```tsx
{event.viewerCanEdit && (
  <PluginLink to="/p/events/$eventId/edit" params={{ eventId: event.id }}>
    <Trans>Edit</Trans>
  </PluginLink>
)}
```

The server already computed access once; the UI just reflects it. Next: how a
page actually calls the backend.
