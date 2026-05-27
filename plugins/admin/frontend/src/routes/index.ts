import { type AnyRoute, createRoute } from '@tanstack/react-router';

import { GroupDetailPage } from './pages/GroupDetailPage.js';
import { GroupsListPage } from './pages/GroupsListPage.js';
import { UserRolesPage } from './pages/UserRolesPage.js';

/** Admin routes (M18 Stage 2). Mounted at `/p/admin`. */
export function buildRoutes(parent: AnyRoute): AnyRoute[] {
  const getParentRoute = () => parent;
  return [
    createRoute({ getParentRoute, path: '/', component: GroupsListPage }),
    createRoute({ getParentRoute, path: '/groups/$groupId', component: GroupDetailPage }),
    createRoute({ getParentRoute, path: '/user-roles', component: UserRolesPage }),
  ];
}
