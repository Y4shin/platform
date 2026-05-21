import type { ComponentType } from 'react';

import { useComponentRegistry } from './ComponentRegistryProvider.js';

/**
 * Look up a cross-plugin component by its `<plugin>.<Component>` key. Returns
 * `undefined` if the producing plugin is disabled (optional-dep semantics) or
 * if the key isn't registered. Consumers should fall back to a sensible
 * default when this returns `undefined`.
 *
 * At M04 the registry is always empty; M09 populates it from manifests.
 */
export function useComponent(key: string): ComponentType<unknown> | undefined {
  const registry = useComponentRegistry();
  return registry[key];
}
