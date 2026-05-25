import { type AnyRoute, createRoute } from '@tanstack/react-router';

import { EventsPage } from './pages/EventsPage.js';
import { PublicInvitePage } from './pages/PublicInvitePage.js';

/**
 * Compose this plugin's authed routes under `parent` (the plugin's mount-prefix
 * route at `/p/events`). Stage 1 ships a single placeholder page; the real
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

/**
 * Compose this plugin's public (login-optional) routes under `parent` (mounted
 * at `/i/events` outside the authed shell). The invite page at
 * `/i/events/<slug>` renders for logged-out visitors.
 */
export function buildPublicRoutes(parent: AnyRoute): AnyRoute[] {
  return [
    createRoute({
      getParentRoute: () => parent,
      path: '/$slug',
      component: PublicInvitePage,
    }),
  ];
}
