import { type AnyRoute, createRoute } from '@tanstack/react-router';

import { WidgetsPage } from './pages/WidgetsPage.js';

/**
 * Compose this plugin's routes under `parent` (the plugin's mount-prefix route
 * created by the shell). The shell concatenates every plugin's `buildRoutes`.
 */
export function buildRoutes(parent: AnyRoute): AnyRoute[] {
  return [
    createRoute({
      getParentRoute: () => parent,
      path: '/',
      component: WidgetsPage,
    }),
  ];
}
