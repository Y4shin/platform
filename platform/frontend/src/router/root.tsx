import type { RouterContext } from '@junius/sdk';
import { createRootRouteWithContext, createRoute, Outlet } from '@tanstack/react-router';

import { Shell } from '../layout/Shell.js';
import { RouteErrorBoundary } from './RouteErrorBoundary.js';

// The bare root renders just an <Outlet/> so public (login-optional) routes
// mounted directly under it (e.g. `/i/<plugin>`) get no authed chrome. Typed
// with the host `RouterContext` ({ user }) so `requirePermissions` route guards
// can read the viewer in `beforeLoad`; the live user is injected per render by
// `<RouterProvider context={{ user }}>` in AppShell.
export const rootRoute = createRootRouteWithContext<RouterContext>()({
  component: () => <Outlet />,
});

// Pathless layout route carrying the authed Shell chrome. The generated route
// tree hangs the index + every `/p/<plugin>` route off this, so the
// authenticated app is wrapped in the Shell while `/i/*` is not. Its
// `errorComponent` is the <RouteErrorBoundary>, so an uncaught render error in
// any authed page shows a friendly card (with a correlation id) instead of a
// white screen.
export const authedLayoutRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'authed',
  errorComponent: RouteErrorBoundary,
  component: () => (
    <Shell>
      <Outlet />
    </Shell>
  ),
});
