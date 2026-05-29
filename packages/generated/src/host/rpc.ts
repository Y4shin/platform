// Host-owned RPC namespace (M18). Mirrors the per-plugin `rpc.ts` barrels for
// the platform's own `user.v1.UserService` — the off-login OIDC-group refresh
// surface. `RefreshOidcGroups` is the frontend self-refresh seam (e.g. a
// "Refresh groups" button on the M19 `/me` page); `ResyncOidcGroups` is the
// admin sweep the `junius oidc resync` CLI drives.
//
// Hand-authored (host services aren't part of `junius sync`'s plugin barrel
// generation); see docs/impl/20-M18-group-role-provisioning.md.

import { UserService } from '../proto/user/v1/user_pb.js';

export const rpc = {
  UserService: {
    refreshOidcGroups: UserService.method.refreshOidcGroups,
    resyncOidcGroups: UserService.method.resyncOidcGroups,
  },
};
