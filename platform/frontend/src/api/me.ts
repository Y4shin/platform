// The `/api/me` payload — the host's `MeResponse` (see platform/src/auth/me.rs):
// the authenticated user flattened, plus the bound OIDC subject, the caller's
// live sessions, and the deployment contact address. Shared by the profile page
// (`/me`) and the dashboard (`/`), which both branch on this one fetch.

export interface MembershipRow {
  groupId: string;
  groupName: string;
  role: { id: string; name: string };
  permissions: string[];
  managedBy: string;
}

export interface UserRoleRow {
  roleId: string;
  roleName: string;
  permissions: string[];
}

export interface SessionRow {
  id: string;
  userAgent: string | null;
  lastSeen: string;
  current: boolean;
}

export interface MeResponse {
  id: string;
  email: string;
  displayName: string;
  locale: string | null;
  oidcSub: string;
  memberships: MembershipRow[];
  userRoles: UserRoleRow[];
  sessions: SessionRow[];
  // `[config] admin_contact_email`, or null when the deployment leaves it unset.
  adminContactEmail: string | null;
}

export const ME_QUERY_KEY = ['me'] as const;

export async function fetchMe(): Promise<MeResponse> {
  const res = await fetch('/api/me', { credentials: 'include' });
  if (!res.ok) {
    throw new Error(`GET /api/me failed: ${res.status}`);
  }
  return (await res.json()) as MeResponse;
}
