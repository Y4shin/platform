import { createRootRoute, createRoute, Outlet } from '@tanstack/react-router';

import { Shell } from '../layout/Shell.js';

// The bare root renders just an <Outlet/> so public (login-optional) routes
// mounted directly under it (e.g. `/i/<plugin>`) get no authed chrome.
export const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

// Pathless layout route carrying the authed Shell chrome. The generated route
// tree hangs the index + every `/p/<plugin>` route off this, so the
// authenticated app is wrapped in the Shell while `/i/*` is not.
export const authedLayoutRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'authed',
  component: () => (
    <Shell>
      <Outlet />
    </Shell>
  ),
});
