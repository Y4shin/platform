import { type AnyRoute, createRoute } from '@tanstack/react-router';

import { HelloPage } from './pages/HelloPage.js';

/**
 * Compose this plugin's routes under `parent` (the plugin's mount-prefix
 * route created by the shell). Each plugin exports a `buildRoutes` of this
 * shape; the shell concatenates them in deployment order.
 */
export function buildRoutes(parent: AnyRoute): AnyRoute[] {
  return [
    createRoute({
      getParentRoute: () => parent,
      path: '/',
      component: HelloPage,
    }),
  ];
}
