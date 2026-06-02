import { type AnyRoute, createRoute } from '@tanstack/react-router';

import { AuditPage } from './pages/AuditPage.js';
import { GroupDetailPage } from './pages/GroupDetailPage.js';
import { GroupsListPage } from './pages/GroupsListPage.js';
import { OidcMappingsPage } from './pages/OidcMappingsPage.js';
import { UserRolesPage } from './pages/UserRolesPage.js';

/** Admin routes (M18 + M19). Mounted at `/p/admin`. */
export function buildRoutes(parent: AnyRoute): AnyRoute[] {
  const getParentRoute = () => parent;
  return [
    createRoute({ getParentRoute, path: '/', component: GroupsListPage }),
    createRoute({ getParentRoute, path: '/audit', component: AuditPage }),
    createRoute({ getParentRoute, path: '/groups/$groupId', component: GroupDetailPage }),
    createRoute({ getParentRoute, path: '/user-roles', component: UserRolesPage }),
    createRoute({ getParentRoute, path: '/oidc', component: OidcMappingsPage }),
  ];
}
