import { type AnyRoute, createRoute } from '@tanstack/react-router';

import { EventDetailPage } from './pages/EventDetailPage.js';
import { EventEditPage } from './pages/EventEditPage.js';
import { EventsListPage } from './pages/EventsListPage.js';
import { InviteManagePage } from './pages/InviteManagePage.js';
import { PublicInvitePage } from './pages/PublicInvitePage.js';

/**
 * Compose this plugin's authed routes under `parent` (mounted at `/p/events`):
 * the list, a new/edit form, the detail page, and the owner invite manager.
 * (`/new` is matched ahead of `/$eventId` as a static segment.)
 */
export function buildRoutes(parent: AnyRoute): AnyRoute[] {
  const getParentRoute = () => parent;
  return [
    createRoute({ getParentRoute, path: '/', component: EventsListPage }),
    createRoute({ getParentRoute, path: '/new', component: EventEditPage }),
    createRoute({ getParentRoute, path: '/$eventId', component: EventDetailPage }),
    createRoute({ getParentRoute, path: '/$eventId/edit', component: EventEditPage }),
    createRoute({ getParentRoute, path: '/$eventId/invite', component: InviteManagePage }),
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
