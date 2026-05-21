import { createRootRoute, Outlet } from '@tanstack/react-router';

import { Shell } from '../layout/Shell.js';

export const rootRoute = createRootRoute({
  component: () => (
    <Shell>
      <Outlet />
    </Shell>
  ),
});
