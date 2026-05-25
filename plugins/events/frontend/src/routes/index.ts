import { type AnyRoute, createRoute } from '@tanstack/react-router';

import { EventsPage } from './pages/EventsPage.js';

/**
 * Compose this plugin's routes under `parent` (the plugin's mount-prefix route
 * created by the shell). Stage 1 ships a single placeholder page; the real
 * list/detail/edit pages land in Stage 9.
 */
export function buildRoutes(parent: AnyRoute): AnyRoute[] {
  return [
    createRoute({
      getParentRoute: () => parent,
      path: '/',
      component: EventsPage,
    }),
  ];
}
