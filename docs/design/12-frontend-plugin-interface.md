# 12. Frontend Plugin Interface

This section defines the TypeScript/React surface a frontend plugin author writes against. It mirrors [11-backend-plugin-interface.md](11-backend-plugin-interface.md)'s structure for the backend, but is meaningfully lighter — TypeScript lacks proc macros, React's component model already handles per-component lifecycle, and most of the contract is "export a fixed shape and `junius` composes."

## 12.1 Overview & philosophy

A frontend plugin is a pnpm workspace package `@junius/plugin-<name>` that exports a fixed surface from `src/index.ts`. There is no `Plugin` interface to implement — the contract is the named exports.

All generated TypeScript code lives in a separate `@junius/generated` package (§12.2). Plugin source trees contain **no generated files** — analogous to the backend's macro-based approach where nothing generated lives inside `plugins/<name>/`.

The shell app (`platform/frontend/`) composes plugins into one React/TanStack Router application. Composition happens at build time via `junius`-generated aggregators that import from each plugin's package.

## 12.2 The `@junius/generated` package

All TS codegen lives in one workspace package with subpath exports per plugin:

```
packages/generated/                 # managed by junius; "do not edit" header
├── package.json                    # subpath exports per plugin (see below)
├── shared/
│   └── proto-types.ts              # cross-plugin proto-generated TS types
└── plugins/
    ├── speakers/
    │   ├── index.ts                # barrel: re-exports rpc, METADATA, Permission
    │   ├── rpc.ts                  # narrow Connect-RPC namespace for this plugin
    │   ├── permissions.ts          # Permission union type
    │   └── metadata.ts             # METADATA constant
    └── events/ ...
```

Subpath exports in `package.json`:

```json
{
  "name": "@junius/generated",
  "exports": {
    "./shared/proto": "./dist/shared/proto-types.js",
    "./speakers":     "./dist/plugins/speakers/index.js",
    "./speakers/rpc": "./dist/plugins/speakers/rpc.js",
    "./events":       "./dist/plugins/events/index.js",
    "./events/rpc":   "./dist/plugins/events/rpc.js"
  }
}
```

Plugin code imports:

```ts
import { rpc, METADATA }   from '@junius/generated/events';
import type { Permission } from '@junius/generated/events';
import type { Speaker }    from '@junius/generated/shared/proto';
```

`junius sync` regenerates this package from every plugin's `plugin.toml`. `junius check` validates:
- Every `@junius/generated/<other-plugin>/*` import in a plugin's source is justified by a manifest dep on that other plugin.
- The package's `exports` field matches the plugin set in the source.

**Why a single generated package**: keeps generated content out of plugin source trees (matching backend's macro approach), reduces sync surface (one package to regenerate), and avoids per-plugin `src/generated/` mixing with hand-written code.

**Note**: there is no `@junius/permissions` aggregate package. The source monorepo doesn't know which plugins are enabled in any deployment ([05-repository-and-deployment-layout.md](05-repository-and-deployment-layout.md) §5.5), so deployment-bound aggregates can't live in the source. Each plugin's `Permission` type is locally scoped.

## 12.3 Plugin source tree & public exports

```
plugins/speakers/frontend/
├── package.json                # @junius/plugin-speakers; depends on @junius/generated, @junius/sdk, @junius/design
├── tsconfig.json
├── src/
│   ├── index.ts                # public exports — all hand-written
│   ├── routes/
│   │   ├── index.ts            # exports buildRoutes(parent)
│   │   └── pages/
│   ├── lib/                    # public components (declared in plugin.toml [exposes.components])
│   ├── hooks/                  # plugin-internal hooks
│   └── types.ts
```

No `src/generated/`. The plugin's `src/index.ts`:

```ts
// plugins/speakers/frontend/src/index.ts

export { buildRoutes } from './routes';

// Public components (must match plugin.toml [exposes.components])
export { SpeakerCard }   from './lib/SpeakerCard';
export { SpeakerPicker } from './lib/SpeakerPicker';

// Domain types other plugins may want
export type { Speaker, SpeakerId } from './types';

// No `rpc` export — consumers import from @junius/generated/<this-plugin>/rpc.
// No `permissions` export — same reason.
```

`junius check` verifies that the named component exports match `plugin.toml`'s `[exposes.components]` declarations.

## 12.4 Routes composition

Each plugin exports a `buildRoutes(parent)` function that takes its mount point and returns a TanStack Router subtree:

```ts
// plugins/speakers/frontend/src/routes/index.ts
import { Route, type AnyRoute } from '@tanstack/react-router';
import { requirePermissions } from '@junius/sdk';
import type { Permission } from '@junius/generated/speakers';
import { SpeakersListPage, SpeakerDetailPage, SpeakerEditPage } from './pages';

export function buildRoutes(parent: AnyRoute) {
  const list = new Route({
    getParentRoute: () => parent,
    path: '/',
    component: SpeakersListPage,
  });

  const detail = new Route({
    getParentRoute: () => parent,
    path: '/$speakerId',
    component: SpeakerDetailPage,
    loader: ({ params }) => /* prefetch via TanStack Query */,
  });

  const edit = new Route({
    getParentRoute: () => parent,
    path: '/$speakerId/edit',
    beforeLoad: ({ context }) => requirePermissions<Permission>(context, ['speakers:write']),
    component: SpeakerEditPage,
  });

  return [list, detail, edit];
}
```

`junius` generates the shell's composed router:

```ts
// platform/frontend/src/generated/routes.ts — generated
import { rootRoute } from '../router/root';
import { buildRoutes as buildSpeakers } from '@junius/plugin-speakers';
import { buildRoutes as buildEvents   } from '@junius/plugin-events';

const speakersParent = new Route({ getParentRoute: () => rootRoute, path: '/p/speakers' });
speakersParent.addChildren(buildSpeakers(speakersParent));

const eventsParent = new Route({ getParentRoute: () => rootRoute, path: '/p/events' });
eventsParent.addChildren(buildEvents(eventsParent));

export const routeTree = rootRoute.addChildren([speakersParent, eventsParent]);
```

## 12.5 Permissions

Each plugin's typed `Permission` union lives in `@junius/generated/<plugin>`:

```ts
// @junius/generated/speakers/permissions.ts (generated)
export type Permission =
  | 'speakers:read'
  | 'speakers:write'
  | 'speakers:book';
```

`@junius/sdk` hooks are generic over the permission union:

```ts
export function useHasPermission<P extends string>(p: P): boolean;
export function useHasAllPermissions<P extends string>(perms: P[]): boolean;
export function useHasAnyPermission<P extends string>(perms: P[]): boolean;
export function requirePermissions<P extends string>(ctx: RouterContext, perms: P[]): void;
```

In plugin code, the caller parameterizes with its own union (or with an imported dep's union for cross-plugin checks):

```ts
import { useHasPermission } from '@junius/sdk';
import type { Permission as MyPerm } from '@junius/generated/speakers';

const canEdit = useHasPermission<MyPerm>('speakers:write');   // ✓
const typo    = useHasPermission<MyPerm>('spekers:write');     // ✗ TS error
```

For cross-plugin permission checks (rare — typically each plugin guards its own):

```ts
// In plugin events, which declared a dep on speakers:
import type { Permission as SpeakersPerm } from '@junius/generated/speakers';
const canBook = useHasPermission<SpeakersPerm>('speakers:book');
```

`junius check` enforces that any `@junius/generated/<other-plugin>` import has a corresponding `[dependencies.<other-plugin>]` declaration in the importer's manifest.

Three places permissions are checked, mirroring the backend's three (extractor / repo / RPC):

1. **Route guards** via `requirePermissions` in `beforeLoad`.
2. **Component-level hooks** (`useHasPermission`).
3. **RPC method calls** — surfaced from `option (platform.requires)` in `.proto` ([11-backend-plugin-interface.md](11-backend-plugin-interface.md) §11.8). v1: server-side rejection only; client-side pre-check is a v2 UX optimization.

The runtime side: the auth context provides `User.permissions: ReadonlySet<string>`. Hooks check set membership. Compile-time safety comes from the typed unions at each call site.

## 12.6 RPC — narrow per-plugin client over a single shell transport

The shell owns the Connect transport. Each plugin gets a generated narrow `rpc` namespace exposing only the methods declared in its manifest (its own services plus cross-plugin methods declared via `[dependencies.<dep>].rpc_methods`).

Manifest declaration:

```toml
# plugins/events/plugin.toml
[dependencies.speakers]
optional     = false
tables       = ["speaker"]
rpc_methods  = ["SpeakerService.GetSpeaker", "SpeakerService.ListSpeakers"]
```

Generated narrow namespace:

```ts
// @junius/generated/events/rpc.ts (generated)
import {
  EventService_GetEvent, EventService_CreateEvent,
  SpeakerService_GetSpeaker, SpeakerService_ListSpeakers,
} from '@junius/generated/shared/proto';

export const rpc = {
  EventService: {
    getEvent:    EventService_GetEvent,
    createEvent: EventService_CreateEvent,
  },
  SpeakerService: {
    // Only the two declared methods. createSpeaker, book, etc. are absent.
    getSpeaker:   SpeakerService_GetSpeaker,
    listSpeakers: SpeakerService_ListSpeakers,
  },
} as const;
```

Plugin code via Connect-Query:

```ts
import { rpc } from '@junius/generated/events';
import { useQuery } from '@connectrpc/connect-query';

function VenueSpeakersList({ venueId }: { venueId: VenueId }) {
  const { data } = useQuery(rpc.SpeakerService.listSpeakers, { venueId });
  // rpc.SpeakerService.createSpeaker is undefined at the type level → compile error
}
```

The shell sets up the Connect transport once. Plugins don't see or construct it — they just call Connect-Query hooks against `rpc.*` method descriptors. The transport handles auth headers, base URL, retries.

This mirrors the backend's repository pattern ([11-backend-plugin-interface.md](11-backend-plugin-interface.md) §11.5): a single source of truth (shell transport + shared proto schemas), with each plugin's compile-time surface narrowed to its declared usage.

**v1 enforcement** of the cross-plugin import allowlist is via `junius check` (convention) scanning imports against each plugin's manifest. Per-plugin `tsconfig.json` `paths` allowlists are an option to upgrade to if drift becomes a problem.

## 12.7 Cross-plugin components

See [08-cross-plugin-composition.md](08-cross-plugin-composition.md) for the full mechanism. FE-specific patterns:

**Required deps**: regular ES imports.

```ts
import { SpeakerPicker } from '@junius/plugin-speakers';
import type { SpeakerId } from '@junius/plugin-speakers';
```

**Optional deps**: typed registry from `@junius/sdk`.

```ts
import { getComponent } from '@junius/sdk';
const VenuePicker = getComponent('venues.VenuePicker'); // typed; `undefined` if disabled
```

The registry is populated by `junius`-generated code in the shell.

## 12.8 Auth & user context

`@junius/sdk` provides:

```ts
interface User {
  id: UserId;
  email: string;
  displayName: string;
  permissions: ReadonlySet<string>;  // populated from the host's RBAC at login
}

export function useUser(): User | null;
export function useIsAuthenticated(): boolean;
export function useCurrentUser(): User;  // throws/redirects if not authenticated
```

The shell wraps the app in an `AuthProvider` that fetches/refreshes the session and handles login/logout. Plugin code never touches auth state directly — only the hooks.

## 12.9 Design system

`@junius/design` package, built on **Tailwind + Radix UI primitives**:

- **Tailwind** for utility-class styling. One `tailwind.config.ts` at the workspace root applied to all plugin source files. `junius sync` maintains the `content` paths to include `plugins/*/frontend/src/**/*.{ts,tsx}`.
- **Radix UI** primitives (`@radix-ui/react-*`) for unstyled accessible behavior (Dialog, Dropdown, Tooltip, Popover, etc.).
- **`@junius/design` wrappers** combine the two into the platform's vocabulary: `Button`, `Input`, `Card`, `Stack`, `Grid`, `Modal`, `Form`. Plugins import from here for any visual component; never write raw `<button>` or HTML form elements directly.

```ts
import { Button, Card, Stack } from '@junius/design';

function SpeakerCard({ speaker }: { speaker: Speaker }) {
  return (
    <Card>
      <Stack gap="md">
        <h3 className="text-lg font-semibold">{speaker.name}</h3>
        <Button variant="primary">View details</Button>
      </Stack>
    </Card>
  );
}
```

Tokens (colors, spacing, typography, radii) are Tailwind theme extensions in the workspace config + exposed as CSS variables for runtime theming. Plugins reference tokens through Tailwind class names (`bg-surface-1`, `text-primary`) — never raw color values. Keeps theming centralized.

## 12.10 Host shell composition

`junius` generates a thin shell entry point with all the providers:

```tsx
// platform/frontend/src/main.tsx (mostly generated)
import { RouterProvider, createRouter } from '@tanstack/react-router';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { TransportProvider } from '@connectrpc/connect-query';
import { createConnectTransport } from '@connectrpc/connect-web';

import { routeTree } from './generated/routes';
import { componentRegistry } from './generated/component-registry';
import { ComponentRegistryProvider, AuthProvider } from '@junius/sdk';

const transport = createConnectTransport({ baseUrl: '/' });
const queryClient = new QueryClient();
const router = createRouter({ routeTree, context: { auth: undefined! } });

function App() {
  return (
    <AuthProvider onReady={(auth) => router.update({ context: { auth } })}>
      <TransportProvider transport={transport}>
        <QueryClientProvider client={queryClient}>
          <ComponentRegistryProvider registry={componentRegistry}>
            <RouterProvider router={router} />
          </ComponentRegistryProvider>
        </QueryClientProvider>
      </TransportProvider>
    </AuthProvider>
  );
}
```

The shell is intentionally tiny — ~50 LoC of bootstrapping. Everything else lives in plugins.

## 12.11 Implementation details still to work out

These don't block the design.

- Exact codegen pipeline for `@junius/generated` (when to regenerate, gitignored vs. committed, hot-reload story in dev).
- pnpm `exports` field templating for many plugins (`junius sync` writes this).
- TypeScript-side enforcement of the cross-plugin import allowlist via per-plugin `tsconfig.json` `paths` — adopt if convention drift becomes a problem.
- Connect-Query method descriptor codegen consistency in `@junius/generated/shared/proto`.
- `AuthProvider` session refresh strategy (cookies, OAuth flows, etc.) — depends on the auth open question ([13-open-questions.md](13-open-questions.md)).
- Component registry type shape for required vs. optional dep keys.
- Tailwind config composition: workspace-root vs. per-plugin theme extensions.
- `Form` component shape: react-hook-form integration? TanStack Form? Defer until the first complex form appears.
- Sub-layout patterns: plugin authoring guide should document the "parent route with `Layout` component" pattern.
