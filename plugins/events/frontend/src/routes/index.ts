import { type RouterContext, requirePermissions } from '@junius/sdk';
import { type AnyRoute, createRoute } from '@tanstack/react-router';

import { EventDetailPage } from './pages/EventDetailPage.js';
import { EventEditPage } from './pages/EventEditPage.js';
import { EventsListPage } from './pages/EventsListPage.js';
import { InviteManagePage } from './pages/InviteManagePage.js';
import { PublicInvitePage } from './pages/PublicInvitePage.js';

// The create/edit form mutates events; gate it on `events:write` so a viewer
// without it is redirected to `/403` (naming the missing permission) before the
// page renders, rather than hitting a `PermissionDenied` only on submit. First
// real consumer of the M19 `requirePermissions` route guard.
const requireWrite = ({ context }: { context: RouterContext }) =>
  requirePermissions(context, ['events:write']);

/**
 * Compose this plugin's authed routes under `parent` (mounted at `/p/events`):
 * the list, a new/edit form, the detail page, and the owner invite manager.
 * (`/new` is matched ahead of `/$eventId` as a static segment.)
 */
export function buildRoutes(parent: AnyRoute): AnyRoute[] {
  const getParentRoute = () => parent;
  return [
    createRoute({ getParentRoute, path: '/', component: EventsListPage }),
    createRoute({
      getParentRoute,
      path: '/new',
      component: EventEditPage,
      beforeLoad: requireWrite,
    }),
    createRoute({ getParentRoute, path: '/$eventId', component: EventDetailPage }),
    createRoute({
      getParentRoute,
      path: '/$eventId/edit',
      component: EventEditPage,
      beforeLoad: requireWrite,
    }),
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
