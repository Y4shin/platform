/**
 * Permission hooks.
 *
 * At M04 these are stubs: `useHasPermission` always returns `true` and
 * `requirePermissions` is a no-op. Real enforcement lands at M07 when the
 * typed permission system goes live. The signature deliberately matches the
 * M07 shape (generic over the plugin's permission union) so consumer code
 * doesn't need to move.
 */

import { useAuth } from '../auth/AuthProvider.js';

export function useHasPermission<P extends string>(_perm: P): boolean {
  // M04: every permission check passes. Real impl reads
  // useAuth().user.permissions in M07.
  useAuth(); // assert provider is present
  return true;
}

export function useHasAllPermissions<P extends string>(_perms: readonly P[]): boolean {
  useAuth();
  return true;
}

export function useHasAnyPermission<P extends string>(_perms: readonly P[]): boolean {
  useAuth();
  return true;
}
